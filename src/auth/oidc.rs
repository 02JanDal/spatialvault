use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use openidconnect::{
    IssuerUrl,
    core::{CoreClient, CoreProviderMetadata},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

use crate::config::OidcConfig;
use crate::error::{AppResult, Config, Internal, Unauthorized};
use snafu::OptionExt;
use tokio::time::timeout;

// Create an async HTTP client for openidconnect
fn http_client() -> Result<openidconnect::reqwest::Client, openidconnect::reqwest::Error> {
    openidconnect::reqwest::ClientBuilder::new().build()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub aud: OneOrMany<String>,
    pub exp: i64,
    pub iat: i64,
    #[serde(default)]
    pub preferred_username: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub groups: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        match self {
            OneOrMany::One(v) => v == value,
            OneOrMany::Many(vs) => vs.contains(value),
        }
    }
}

/// Response from the OIDC token introspection endpoint (RFC 7662)
#[derive(Debug, Deserialize)]
struct IntrospectionResponse {
    active: bool,
    #[serde(default)]
    sub: Option<String>,
    #[serde(default)]
    exp: Option<i64>,
    #[serde(default)]
    iat: Option<i64>,
    #[serde(default)]
    aud: Option<OneOrMany<String>>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    groups: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct OidcValidator {
    config: OidcConfig,
    jwks: Arc<RwLock<Option<jsonwebtoken::jwk::JwkSet>>>,
    client: Option<CoreClient>,
    introspection_url: Option<String>,
    http_client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, (Claims, i64)>>>,
}

/// Check if a token looks like a JWT (three Base64URL segments separated by dots)
fn is_jwt(token: &str) -> bool {
    token.chars().filter(|c| *c == '.').count() == 2
}

impl OidcValidator {
    pub async fn new(config: OidcConfig) -> AppResult<Self> {
        let issuer_url = IssuerUrl::new(config.issuer_url.clone()).map_err(|e| {
            Config {
                message: format!("Invalid issuer URL: {}", e),
            }
            .build()
        })?;

        // Create HTTP client
        let client = http_client().map_err(|e| {
            Config {
                message: format!("Failed to create HTTP client: {}", e),
            }
            .build()
        })?;

        // Discover OIDC provider metadata
        let provider_metadata = timeout(
            Duration::from_millis(2_000),
            CoreProviderMetadata::discover_async(issuer_url, &client),
        )
        .await
        .map_err(|e| {
            Config {
                message: format!("OIDC discovery timed out: {}", e),
            }
            .build()
        })?
        .map_err(|e| {
            Config {
                message: format!("OIDC discovery failed: {}", e),
            }
            .build()
        })?;

        // Fetch JWKS
        let jwks_uri = provider_metadata.jwks_uri();

        let jwks_response = reqwest::get(jwks_uri.as_str()).await.map_err(|e| {
            Config {
                message: format!("Failed to fetch JWKS: {}", e),
            }
            .build()
        })?;

        let jwks: jsonwebtoken::jwk::JwkSet = jwks_response.json().await.map_err(|e| {
            Config {
                message: format!("Failed to parse JWKS: {}", e),
            }
            .build()
        })?;

        // Fetch introspection endpoint from discovery document
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            config.issuer_url.trim_end_matches('/')
        );
        let introspection_url = async {
            let resp = reqwest::get(&discovery_url).await.ok()?;
            let json: serde_json::Value = resp.json().await.ok()?;
            json.get("introspection_endpoint")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        }
        .await;

        if let Some(ref url) = introspection_url {
            tracing::info!("Token introspection endpoint discovered: {}", url);
        }

        Ok(Self {
            config,
            jwks: Arc::new(RwLock::new(Some(jwks))),
            client: None,
            introspection_url,
            http_client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a validator for testing (no OIDC discovery)
    pub fn new_for_testing(config: OidcConfig) -> Self {
        Self {
            config,
            jwks: Arc::new(RwLock::new(None)),
            client: None,
            introspection_url: None,
            http_client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn validate_token(&self, token: &str) -> AppResult<Claims> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some((claims, exp)) = cache.get(token) {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                if now < *exp {
                    return Ok(claims.clone());
                }
            }
        }

        // Detect token type and validate accordingly
        let claims = if is_jwt(token) {
            self.validate_jwt(token).await?
        } else {
            self.introspect_token(token).await?
        };

        // Cache the result (keyed by token string, expires at token's exp time)
        {
            let mut cache = self.cache.write().await;
            cache.insert(token.to_string(), (claims.clone(), claims.exp));
        }

        Ok(claims)
    }

    /// Validate a JWT token using the cached JWKS
    async fn validate_jwt(&self, token: &str) -> AppResult<Claims> {
        let header = decode_header(token).map_err(|e| {
            Unauthorized {
                message: format!("Invalid token header: {}", e),
            }
            .build()
        })?;

        let kid = header.kid.context(Unauthorized {
            message: "Token missing kid".to_string(),
        })?;

        let jwks = self.jwks.read().await;
        let jwks = jwks.as_ref().context(Internal {
            message: "JWKS not initialized".to_string(),
        })?;

        let jwk = jwks.find(&kid).context(Unauthorized {
            message: format!("Unknown key id: {}", kid),
        })?;

        let decoding_key = DecodingKey::from_jwk(jwk).map_err(|e| {
            Internal {
                message: format!("Failed to create decoding key: {}", e),
            }
            .build()
        })?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.config.audience]);
        validation.set_issuer(&[&self.config.issuer_url]);

        let token_data = decode::<Claims>(token, &decoding_key, &validation).map_err(|e| {
            Unauthorized {
                message: format!("Token validation failed: {}", e),
            }
            .build()
        })?;

        Ok(token_data.claims)
    }

    /// Introspect an opaque token using the OIDC token introspection endpoint (RFC 7662)
    async fn introspect_token(&self, token: &str) -> AppResult<Claims> {
        let introspection_url = self.introspection_url.as_ref().ok_or_else(|| {
            Unauthorized {
                message: "Token introspection is not configured; opaque tokens are not supported"
                    .to_string(),
            }
            .build()
        })?;

        let (client_id, client_secret) = match (&self.config.client_id, &self.config.client_secret)
        {
            (Some(id), Some(secret)) => (id, secret),
            _ => {
                return Err(Unauthorized {
                    message: "Client credentials required for token introspection".to_string(),
                }
                .build());
            }
        };

        let response = self
            .http_client
            .post(introspection_url)
            .basic_auth(client_id, Some(client_secret))
            .form(&[("token", token)])
            .send()
            .await
            .map_err(|e| {
                Internal {
                    message: format!("Token introspection request failed: {}", e),
                }
                .build()
            })?;

        if !response.status().is_success() {
            return Err(Internal {
                message: format!("Token introspection returned status {}", response.status()),
            }
            .build());
        }

        let introspection: IntrospectionResponse = response.json().await.map_err(|e| {
            Internal {
                message: format!("Failed to parse introspection response: {}", e),
            }
            .build()
        })?;

        if !introspection.active {
            return Err(Unauthorized {
                message: "Token is not active".to_string(),
            }
            .build());
        }

        let sub = introspection.sub.ok_or_else(|| {
            Unauthorized {
                message: "Introspection response missing 'sub' claim".to_string(),
            }
            .build()
        })?;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        Ok(Claims {
            sub,
            aud: introspection
                .aud
                .unwrap_or_else(|| OneOrMany::One(self.config.audience.clone())),
            exp: introspection.exp.unwrap_or(now + 300),
            iat: introspection.iat.unwrap_or(now),
            preferred_username: introspection.preferred_username,
            email: introspection.email,
            groups: introspection.groups,
        })
    }

    /// Get the username from claims (preferred_username, email, or sub)
    pub fn extract_username(claims: &Claims) -> String {
        claims
            .preferred_username
            .clone()
            .or_else(|| claims.email.clone())
            .unwrap_or_else(|| claims.sub.clone())
    }
}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub username: String,
    pub subject: String,
    pub groups: Vec<String>,
}

impl AuthenticatedUser {
    pub fn from_claims(claims: &Claims) -> Self {
        Self {
            username: OidcValidator::extract_username(claims),
            subject: claims.sub.clone(),
            groups: claims.groups.clone().unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_jwt() {
        // JWTs have exactly two dots (header.payload.signature)
        assert!(is_jwt("eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.signature"));
        assert!(is_jwt("a.b.c"));

        // Opaque tokens don't have exactly two dots
        assert!(!is_jwt("opaque-token-no-dots"));
        assert!(!is_jwt("one.dot"));
        assert!(!is_jwt("three.dots.in.token"));
        assert!(!is_jwt(""));
    }
}

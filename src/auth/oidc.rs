use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use openidconnect::{
    IssuerUrl,
    core::{CoreClient, CoreProviderMetadata},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
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

#[derive(Clone)]
pub struct OidcValidator {
    config: OidcConfig,
    jwks: Arc<RwLock<Option<jsonwebtoken::jwk::JwkSet>>>,
    client: Option<CoreClient>,
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

        Ok(Self {
            config,
            jwks: Arc::new(RwLock::new(Some(jwks))),
            client: None, // Client not needed for token validation
        })
    }

    /// Create a validator for testing (no OIDC discovery)
    pub fn new_for_testing(config: OidcConfig) -> Self {
        Self {
            config,
            jwks: Arc::new(RwLock::new(None)),
            client: None,
        }
    }

    pub async fn validate_token(&self, token: &str) -> AppResult<Claims> {
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

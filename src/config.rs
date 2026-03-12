use serde::Deserialize;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    pub oidc: Option<OidcConfig>,
    pub s3: Option<S3Config>,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Regex pattern for allowed CORS origins. If omitted, all origins are allowed.
    /// Example: `https?://(mydomain|myotherdomain)\.com`
    pub cors_origins: Option<String>,
}

// Custom Debug implementation to prevent secrets from being logged
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("auth", &self.auth)
            .field("oidc", &self.oidc)
            .field("s3", &self.s3)
            .field("base_url", &self.base_url)
            .field("cors_origins", &self.cors_origins)
            .finish()
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_base_url() -> String {
    "http://localhost:8080".to_string()
}

#[derive(Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

// Custom Debug implementation to redact database URL (may contain password)
impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .finish()
    }
}

fn default_max_connections() -> u32 {
    10
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub disabled: bool,
    /// Allow `Authorization: User <username>` headers to bypass OIDC.
    /// For local development only — never enable in production.
    #[serde(default)]
    pub dev_auth: bool,
}

#[derive(Clone, Deserialize)]
pub struct OidcConfig {
    pub issuer_url: String,
    #[serde(default = "default_audience")]
    pub audience: String,
    /// Client ID for token introspection (required for opaque token support)
    #[serde(default)]
    pub client_id: Option<String>,
    /// Client secret for token introspection (required for opaque token support)
    #[serde(default)]
    pub client_secret: Option<String>,
}

// Custom Debug implementation to redact client_secret
impl fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OidcConfig")
            .field("issuer_url", &self.issuer_url)
            .field("audience", &self.audience)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

fn default_audience() -> String {
    "spatialvault".to_string()
}

#[derive(Clone, Deserialize)]
pub struct S3Config {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub access_key_id: Option<String>,
    #[serde(default)]
    pub secret_access_key: Option<String>,
}

// Custom Debug implementation to redact S3 credentials
impl fmt::Debug for S3Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field(
                "access_key_id",
                &self.access_key_id.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "secret_access_key",
                &self.secret_access_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

impl Config {
    /// Returns the path prefix to mount the API under, derived from `base_url`.
    /// Returns `None` if `base_url` has no meaningful path (i.e. root `/`).
    pub fn path_prefix(&self) -> Option<String> {
        match url::Url::parse(&self.base_url) {
            Err(e) => {
                tracing::warn!("Failed to parse base_url '{}': {}", self.base_url, e);
                None
            }
            Ok(u) => {
                let path = u.path().trim_end_matches('/').to_string();
                if path.is_empty() { None } else { Some(path) }
            }
        }
    }

    pub fn load() -> Result<Arc<Self>, config::ConfigError> {
        let config = config::Config::builder()
            .add_source(config::File::with_name("config").required(false))
            .add_source(
                config::Environment::with_prefix("SPATIALVAULT")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        let settings: Config = config.try_deserialize()?;
        Ok(Arc::new(settings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        assert_eq!(default_host(), "0.0.0.0");
        assert_eq!(default_port(), 8080);
    }

    #[test]
    fn test_path_prefix() {
        let mut cfg = Config {
            host: default_host(),
            port: default_port(),
            database: DatabaseConfig {
                url: "postgres://localhost/test".to_string(),
                max_connections: 5,
            },
            auth: AuthConfig::default(),
            oidc: None,
            s3: None,
            base_url: "http://localhost:8080".to_string(),
            cors_origins: None,
        };
        assert_eq!(cfg.path_prefix(), None);

        cfg.base_url = "https://example.com/spatialvault".to_string();
        assert_eq!(cfg.path_prefix(), Some("/spatialvault".to_string()));

        cfg.base_url = "https://example.com/spatialvault/".to_string();
        assert_eq!(cfg.path_prefix(), Some("/spatialvault".to_string()));

        cfg.base_url = "https://example.com/".to_string();
        assert_eq!(cfg.path_prefix(), None);
    }
}

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
    pub oidc: OidcConfig,
    #[serde(default)]
    pub s3: S3Config,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

// Custom Debug implementation to prevent secrets from being logged
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("oidc", &self.oidc)
            .field("s3", &self.s3)
            .field("base_url", &self.base_url)
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
    #[serde(default = "default_service_role")]
    pub service_role: String,
}

// Custom Debug implementation to redact database URL (may contain password)
impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .field("service_role", &self.service_role)
            .finish()
    }
}

fn default_max_connections() -> u32 {
    10
}

fn default_service_role() -> String {
    "spatialvault_service".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct OidcConfig {
    pub issuer_url: String,
    #[serde(default = "default_audience")]
    pub audience: String,
}

fn default_audience() -> String {
    "spatialvault".to_string()
}

#[derive(Clone, Default, Deserialize)]
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
            .field("access_key_id", &self.access_key_id.as_ref().map(|_| "[REDACTED]"))
            .field("secret_access_key", &self.secret_access_key.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl Config {
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
        assert_eq!(default_service_role(), "spatialvault_service");
    }
}

use sqlx::{
    Executor,
    postgres::{PgPool, PgPoolOptions},
};
use std::sync::Arc;

use crate::config::DatabaseConfig;
use crate::error::{AppResult, BadRequest, Internal};

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(config: &DatabaseConfig) -> AppResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.url)
            .await?;

        Ok(Self {
            pool,
        })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Execute SQL with a specific role context using a transaction
    pub async fn execute_as(&self, username: &str, sql: &str) -> AppResult<()> {
        let mut tx = self.begin_as(username).await?;
        tx.execute(sql).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Begin a transaction with the role set to the specified user.
    /// All queries executed on this transaction will run as the user,
    /// allowing PostgreSQL to enforce permission checks.
    pub async fn begin_as(
        &self,
        username: &str,
    ) -> AppResult<sqlx::Transaction<'_, sqlx::Postgres>> {
        if !is_valid_role_name(username) {
            return Err(BadRequest { message: format!(
                "Invalid username: {}",
                username
            ) }.build());
        }

        let mut tx = self.pool.begin().await?;
        tx.execute(format!("SET LOCAL ROLE {}", quote_ident(username)).as_str())
            .await?;
        Ok(tx)
    }

    pub async fn run_migrations(&self) -> AppResult<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| Internal { message: format!("Migration failed: {}", e) }.build())?;
        Ok(())
    }
}

/// Validate that a role name is safe to use in SQL
fn is_valid_role_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && name
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
}

/// Quote an identifier for safe use in SQL
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub type DbPool = Arc<Database>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_role_names() {
        assert!(is_valid_role_name("jan"));
        assert!(is_valid_role_name("user_name"));
        assert!(is_valid_role_name("user-name"));
        assert!(is_valid_role_name("_private"));
        assert!(is_valid_role_name("User123"));
    }

    #[test]
    fn test_invalid_role_names() {
        assert!(!is_valid_role_name(""));
        assert!(!is_valid_role_name("123user"));
        assert!(!is_valid_role_name("user;drop"));
        assert!(!is_valid_role_name("user'name"));
        assert!(!is_valid_role_name("user name"));
    }

    #[test]
    fn test_quote_ident() {
        assert_eq!(quote_ident("simple"), "\"simple\"");
        assert_eq!(quote_ident("with\"quote"), "\"with\"\"quote\"");
    }
}

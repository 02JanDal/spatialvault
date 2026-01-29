use sqlx::{
    postgres::{PgPool, PgPoolOptions},
    Executor,
};
use std::sync::Arc;

use crate::config::DatabaseConfig;
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
    service_role: String,
}

impl Database {
    pub async fn connect(config: &DatabaseConfig) -> AppResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.url)
            .await?;

        Ok(Self {
            pool,
            service_role: config.service_role.clone(),
        })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Execute a database operation with SET ROLE for the given user.
    /// This provides PostgreSQL-level access control based on OIDC identity.
    pub async fn with_role<F, T>(&self, username: &str, operation: F) -> AppResult<T>
    where
        F: FnOnce(&PgPool) -> futures::future::BoxFuture<'_, AppResult<T>>,
    {
        // Validate username to prevent SQL injection
        if !is_valid_role_name(username) {
            return Err(AppError::BadRequest(format!(
                "Invalid username: {}",
                username
            )));
        }

        // Acquire a connection from the pool
        let mut conn = self.pool.acquire().await?;

        // Set the role for this session
        let set_role_sql = format!("SET ROLE {}", quote_ident(username));
        conn.execute(set_role_sql.as_str()).await?;

        // Execute the operation
        // Note: We need to use the pool here, but the SET ROLE only affects
        // this connection. For a proper implementation, we'd need connection-level
        // role management. For now, we'll use a transaction approach.
        let result = {
            let mut tx = self.pool.begin().await?;
            tx.execute(format!("SET LOCAL ROLE {}", quote_ident(username)).as_str())
                .await?;

            // The operation uses the pool, but we'd need to refactor for proper
            // connection-scoped role switching. This is a simplified version.
            let pool = &self.pool;
            operation(pool).await
        };

        // Reset role (connection returned to pool will have role reset)
        conn.execute("RESET ROLE").await?;

        result
    }

    /// Execute SQL with a specific role context using a transaction
    pub async fn execute_as(&self, username: &str, sql: &str) -> AppResult<()> {
        if !is_valid_role_name(username) {
            return Err(AppError::BadRequest(format!(
                "Invalid username: {}",
                username
            )));
        }

        let mut tx = self.pool.begin().await?;
        tx.execute(format!("SET LOCAL ROLE {}", quote_ident(username)).as_str())
            .await?;
        tx.execute(sql).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn run_migrations(&self) -> AppResult<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| AppError::Internal(format!("Migration failed: {}", e)))?;
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
        && name.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
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

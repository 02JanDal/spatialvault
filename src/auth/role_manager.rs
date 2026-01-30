use sqlx::PgPool;

use crate::error::{AppError, AppResult};

/// Manages PostgreSQL roles for users and groups
pub struct RoleManager<'a> {
    pool: &'a PgPool,
}

impl<'a> RoleManager<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Ensure a user role exists, creating it if necessary
    pub async fn ensure_user_role(&self, username: &str) -> AppResult<()> {
        if !is_valid_role_name(username) {
            return Err(AppError::BadRequest(format!(
                "Invalid username: {}",
                username
            )));
        }

        // Call the stored function to create role and schema
        sqlx::query("SELECT spatialvault.ensure_role($1, false)")
            .bind(username)
            .execute(self.pool)
            .await?;

        tracing::info!("Ensured user role exists: {}", username);
        Ok(())
    }

    /// Ensure a group role exists, creating it if necessary
    pub async fn ensure_group_role(&self, group_name: &str) -> AppResult<()> {
        if !is_valid_role_name(group_name) {
            return Err(AppError::BadRequest(format!(
                "Invalid group name: {}",
                group_name
            )));
        }

        sqlx::query("SELECT spatialvault.ensure_role($1, true)")
            .bind(group_name)
            .execute(self.pool)
            .await?;

        tracing::info!("Ensured group role exists: {}", group_name);
        Ok(())
    }

    /// Grant a role to a user (for group membership)
    pub async fn grant_role_to_user(&self, role: &str, user: &str) -> AppResult<()> {
        if !is_valid_role_name(role) || !is_valid_role_name(user) {
            return Err(AppError::BadRequest(
                "Invalid role or user name".to_string(),
            ));
        }

        let sql = format!("GRANT {} TO {}", quote_ident(role), quote_ident(user));
        sqlx::query(&sql).execute(self.pool).await?;

        tracing::info!("Granted role {} to user {}", role, user);
        Ok(())
    }

    /// Revoke a role from a user
    pub async fn revoke_role_from_user(&self, role: &str, user: &str) -> AppResult<()> {
        if !is_valid_role_name(role) || !is_valid_role_name(user) {
            return Err(AppError::BadRequest(
                "Invalid role or user name".to_string(),
            ));
        }

        let sql = format!("REVOKE {} FROM {}", quote_ident(role), quote_ident(user));
        sqlx::query(&sql).execute(self.pool).await?;

        tracing::info!("Revoked role {} from user {}", role, user);
        Ok(())
    }

    /// Check if a role exists
    pub async fn role_exists(&self, role_name: &str) -> AppResult<bool> {
        let result: (bool,) =
            sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname = $1)")
                .bind(role_name)
                .fetch_one(self.pool)
                .await?;

        Ok(result.0)
    }

    /// Grant table privileges to a role
    pub async fn grant_table_privileges(
        &self,
        schema: &str,
        table: &str,
        role: &str,
        privileges: &[&str],
    ) -> AppResult<()> {
        if !is_valid_role_name(schema) || !is_valid_role_name(role) {
            return Err(AppError::BadRequest(
                "Invalid schema or role name".to_string(),
            ));
        }

        // First grant USAGE on the schema so the role can access tables in it
        let schema_sql = format!(
            "GRANT USAGE ON SCHEMA {} TO {}",
            quote_ident(schema),
            quote_ident(role)
        );
        sqlx::query(&schema_sql).execute(self.pool).await?;

        // Then grant the table privileges
        let privs = privileges.join(", ");
        let sql = format!(
            "GRANT {} ON {}.{} TO {}",
            privs,
            quote_ident(schema),
            quote_ident(table),
            quote_ident(role)
        );
        sqlx::query(&sql).execute(self.pool).await?;

        Ok(())
    }

    /// Revoke table privileges from a role
    pub async fn revoke_table_privileges(
        &self,
        schema: &str,
        table: &str,
        role: &str,
    ) -> AppResult<()> {
        if !is_valid_role_name(schema) || !is_valid_role_name(role) {
            return Err(AppError::BadRequest(
                "Invalid schema or role name".to_string(),
            ));
        }

        let sql = format!(
            "REVOKE ALL ON {}.{} FROM {}",
            quote_ident(schema),
            quote_ident(table),
            quote_ident(role)
        );
        sqlx::query(&sql).execute(self.pool).await?;

        Ok(())
    }
}

/// Validate role/schema/table names to prevent SQL injection
pub fn is_valid_role_name(name: &str) -> bool {
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

/// Quote an identifier for safe SQL construction
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

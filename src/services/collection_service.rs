use std::sync::Arc;
use uuid::Uuid;

use crate::api::collections::schemas::{CollectionSchema, ColumnDef, ColumnType};
use crate::api::collections::sharing::{PermissionLevel, ShareEntry};
use crate::api::common::etag::VersionMatch;
use crate::api::common::{Bbox, Extent, SpatialExtent, TemporalExtent};
use crate::auth::{RoleManager, is_valid_role_name, quote_ident};
use crate::db::{Collection, CollectionWithCrs, Database};
use crate::error::{
    AppError, AppResult, BadRequest, Forbidden, NotFound, PreconditionFailed, RenamedTo,
};
use snafu::OptionExt;

/// System columns that are always present in item tables
pub const SYSTEM_COLUMNS: &[&str] = &[
    "_id",
    "geometry",
    "_version",
    "_created_at",
    "_updated_at",
    "_datetime",
];

/// Information about a user-defined column
pub struct ColumnInfo {
    pub name: String,
    pub pg_type: String,
    pub is_nullable: bool,
    pub column_default: Option<String>,
}

/// Map a ColumnType to its PostgreSQL type string
fn pg_type(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::String => "TEXT",
        ColumnType::Integer => "BIGINT",
        ColumnType::Real => "DOUBLE PRECISION",
        ColumnType::Date => "DATE",
        ColumnType::Datetime => "TIMESTAMPTZ",
        ColumnType::Boolean => "BOOLEAN",
    }
}

/// Validate a user-defined column name
fn validate_column_name(name: &str) -> AppResult<()> {
    if name.starts_with('_') {
        return Err(BadRequest {
            message: format!(
                "Column name '{}' cannot start with '_' (reserved for system columns)",
                name
            ),
        }
        .build());
    }
    if name == "geometry" {
        return Err(BadRequest {
            message: "Column name 'geometry' is reserved".to_string(),
        }
        .build());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() {
        return Err(BadRequest {
            message: format!("Invalid column name '{}': must be non-empty and contain only alphanumeric characters and underscores", name),
        }.build());
    }
    Ok(())
}

/// Convert a JSON default value to a SQL DEFAULT expression
fn default_to_sql(value: &serde_json::Value, col_type: &ColumnType) -> AppResult<String> {
    match value {
        serde_json::Value::String(s) if s == "now" && *col_type == ColumnType::Datetime => {
            Ok("NOW()".to_string())
        }
        serde_json::Value::String(s) => Ok(format!("'{}'", s.replace('\'', "''"))),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.to_string()),
        serde_json::Value::Null => Ok("NULL".to_string()),
        _ => Err(BadRequest {
            message: format!("Unsupported default value: {}", value),
        }
        .build()),
    }
}

fn create_assets_table_sql(schema_name: &str, table_name: &str) -> String {
    let quoted_schema = quote_ident(schema_name);
    let quoted_table = quote_ident(table_name);
    let quoted_assets_table = quote_ident(&format!("_{}_assets", table_name));
    format!(
        r#"
        CREATE TABLE IF NOT EXISTS {quoted_schema}.{quoted_assets_table} (
            item_id UUID NOT NULL REFERENCES {quoted_schema}.{quoted_table}(_id) ON DELETE CASCADE,
            PRIMARY KEY (item_id, key)
        ) INHERITS (spatialvault.assets_base)
        "#,
    )
}

/// Parse a canonical collection name into (schema_name, table_name).
/// Returns an error if the name has fewer than two colon-separated segments
/// or if either derived name fails validation.
fn parse_canonical_name(canonical_name: &str) -> AppResult<(String, String)> {
    let parts: Vec<&str> = canonical_name.split(':').collect();
    let schema_name = parts.first().context(BadRequest {
        message: "Invalid collection name".to_string(),
    })?;
    let table_name = parts[1..].join("_");

    if table_name.is_empty() {
        return Err(BadRequest {
            message: "Collection name must have at least two segments".to_string(),
        }
        .build());
    }
    if !is_valid_role_name(schema_name) {
        return Err(BadRequest {
            message: format!("Invalid schema name: {}", schema_name),
        }
        .build());
    }
    if !is_valid_role_name(&table_name) {
        return Err(BadRequest {
            message: format!("Invalid table name: {}", table_name),
        }
        .build());
    }

    Ok((schema_name.to_string(), table_name))
}

pub struct CollectionService {
    db: Arc<Database>,
    base_url: String,
}

impl CollectionService {
    pub fn new(db: Arc<Database>, base_url: String) -> Self {
        Self { db, base_url }
    }

    pub async fn list_collections(
        &self,
        username: &str,
        limit: u32,
        offset: u32,
    ) -> AppResult<(Vec<CollectionWithCrs>, i64)> {
        // List collections accessible to this user with storage CRS included
        // This includes owned collections and shared collections
        let where_clause = r#"
            WHERE c.owner = $1
               OR EXISTS (
                   SELECT 1 FROM information_schema.table_privileges tp
                   WHERE tp.table_schema = c.schema_name
                     AND tp.table_name = c.table_name
                     AND tp.grantee = $1
                     AND tp.privilege_type = 'SELECT'
               )
        "#;

        let count_sql = format!(
            "SELECT COUNT(*) FROM spatialvault.collections c {}",
            where_clause
        );
        let count: (i64,) = sqlx::query_as(&count_sql)
            .bind(username)
            .fetch_one(self.db.pool())
            .await?;

        let select_sql = format!(
            r#"
            SELECT c.*,
                COALESCE(
                    (SELECT srid FROM geometry_columns
                     WHERE f_table_schema = c.schema_name
                     AND f_table_name = c.table_name
                     AND f_geometry_column = 'geometry'
                     LIMIT 1),
                    4326
                ) as storage_crs
            FROM spatialvault.collections c
            {}
            ORDER BY c.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            where_clause
        );
        let collections: Vec<CollectionWithCrs> = sqlx::query_as(&select_sql)
            .bind(username)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(self.db.pool())
            .await?;

        Ok((collections, count.0))
    }

    pub async fn get_collection(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<CollectionWithCrs> {
        // Get collection with authorization check - user must be owner or have SELECT privilege
        let collection: Option<CollectionWithCrs> = sqlx::query_as(
            r#"
            SELECT c.*,
                COALESCE(
                    (SELECT srid FROM geometry_columns
                     WHERE f_table_schema = c.schema_name
                     AND f_table_name = c.table_name
                     AND f_geometry_column = 'geometry'
                     LIMIT 1),
                    4326
                ) as storage_crs
            FROM spatialvault.collections c
            WHERE canonical_name = $1
              AND (
                  c.owner = $2
                  OR EXISTS (
                      SELECT 1 FROM information_schema.table_privileges tp
                      WHERE tp.table_schema = c.schema_name
                        AND tp.table_name = c.table_name
                        AND tp.grantee = $2
                        AND tp.privilege_type = 'SELECT'
                  )
              )
            "#,
        )
        .bind(collection_id)
        .bind(username)
        .fetch_optional(self.db.pool())
        .await?;

        // check if a collection had this name previously
        if collection.is_none() {
            self.check_alias(collection_id).await?;
        }

        if let Some(collection) = collection {
            Ok(collection)
        } else {
            Err(NotFound {
                message: format!("Collection not found: {}", collection_id),
            }
            .build())
        }
    }

    /// Check if there has previously existed a collection by this name
    async fn check_alias(&self, collection_id: &str) -> Result<(), AppError> {
        let new_name: Option<String> = sqlx::query_scalar(
            "SELECT new_name FROM spatialvault.collection_aliases WHERE old_name = $1",
        )
        .bind(collection_id)
        .fetch_optional(self.db.pool())
        .await?;
        if let Some(name) = new_name {
            Err(RenamedTo {
                message: format!("{}/collections/{}", self.base_url, name),
            }
            .build())
        } else {
            Ok(())
        }
    }

    /// Check if user has SELECT privilege on a collection's table.
    async fn has_select_privilege(
        conn: &mut sqlx::PgConnection,
        username: &str,
        collection: &Collection,
    ) -> AppResult<bool> {
        let result: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM information_schema.table_privileges
                WHERE table_schema = $1
                  AND table_name = $2
                  AND grantee = $3
                  AND privilege_type = 'SELECT'
            )
            "#,
        )
        .bind(&collection.schema_name)
        .bind(&collection.table_name)
        .bind(username)
        .fetch_one(&mut *conn)
        .await?;
        Ok(result.unwrap_or(false))
    }

    /// Check ownership, returning NotFound if user has no visibility, Forbidden if user can see but doesn't own.
    fn check_ownership(
        &self,
        username: &str,
        collection: &Collection,
        has_select: bool,
    ) -> AppResult<()> {
        if collection.owner == username {
            return Ok(());
        }
        if has_select {
            // User can see the collection but is not the owner
            return Err(Forbidden {
                message: "Only owner can modify collection".to_string(),
            }
            .build());
        }
        // User cannot see the collection at all
        Err(NotFound {
            message: format!("Collection not found: {}", collection.canonical_name),
        }
        .build())
    }

    /// Check if the user has write permission on a collection
    /// Returns true if user is owner or has INSERT privilege
    pub async fn has_write_permission(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<bool> {
        let result: Option<(bool,)> = sqlx::query_as(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM spatialvault.collections c
                WHERE c.canonical_name = $1
                  AND (
                      c.owner = $2
                      OR EXISTS (
                          SELECT 1 FROM information_schema.table_privileges tp
                          WHERE tp.table_schema = c.schema_name
                            AND tp.table_name = c.table_name
                            AND tp.grantee = $2
                            AND tp.privilege_type = 'INSERT'
                      )
                  )
            )
            "#,
        )
        .bind(collection_id)
        .bind(username)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(result.map(|(exists,)| exists).unwrap_or(false))
    }

    pub async fn get_alias(&self, name: &str) -> AppResult<Option<String>> {
        let alias: Option<(String,)> = sqlx::query_as(
            "SELECT new_name FROM spatialvault.collection_aliases WHERE old_name = $1",
        )
        .bind(name)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(alias.map(|(new_name,)| new_name))
    }

    pub async fn create_collection(
        &self,
        id: Uuid,
        username: &str,
        canonical_name: &str,
        owner: &str,
        title: &str,
        description: Option<&str>,
        collection_type: &str,
        crs: i32,
        columns: Option<&[ColumnDef]>,
        import_job: Option<(&str, &str, &serde_json::Value)>,
    ) -> AppResult<Collection> {
        // Ensure user role exists
        let role_manager = RoleManager::new(self.db.pool());
        role_manager.ensure_user_role(owner).await?;

        // Parse canonical name to get schema and table name
        let (schema_name, table_name) = parse_canonical_name(canonical_name)?;
        let schema_name = schema_name.as_str();

        // Start transaction
        let mut tx = self.db.pool().begin().await?;

        // Insert collection metadata
        let collection: Collection = sqlx::query_as(
            r#"
            INSERT INTO spatialvault.collections
            (id, canonical_name, owner, schema_name, table_name, collection_type, title, description)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(canonical_name)
        .bind(owner)
        .bind(schema_name)
        .bind(&table_name)
        .bind(collection_type)
        .bind(title)
        .bind(description)
        .fetch_one(&mut *tx)
        .await?;

        // Use quote_ident for safe identifier quoting (belt and suspenders with validation)
        let quoted_schema = quote_ident(schema_name);
        let quoted_table = quote_ident(&table_name);

        // Build user column definitions
        let mut user_column_defs = String::new();
        if let Some(cols) = columns {
            for col in cols {
                validate_column_name(&col.name)?;
                let col_type = pg_type(&col.column_type);
                let nullable = if col.nullable { "" } else { " NOT NULL" };
                let default = if let Some(ref def) = col.default {
                    format!(" DEFAULT {}", default_to_sql(def, &col.column_type)?)
                } else {
                    String::new()
                };
                user_column_defs.push_str(&format!(
                    "\n                    {} {}{}{},",
                    quote_ident(&col.name),
                    col_type,
                    nullable,
                    default
                ));
            }
        }

        let create_table_sql = format!(
            r#"
                CREATE TABLE {quoted_schema}.{quoted_table} (
                    _id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    geometry geometry(Geometry, {crs}) NOT NULL,
                    {user_column_defs}
                    _version BIGINT NOT NULL DEFAULT 1,
                    _created_at TIMESTAMPTZ DEFAULT NOW(),
                    _updated_at TIMESTAMPTZ DEFAULT NOW()
                )
                "#,
        );
        sqlx::query(&create_table_sql).execute(&mut *tx).await?;

        // Create spatial index
        let create_index_sql = format!(
            r#"CREATE INDEX ON {}.{} USING GIST(geometry)"#,
            quoted_schema, quoted_table
        );
        sqlx::query(&create_index_sql).execute(&mut *tx).await?;

        // Set table ownership to the owner role
        let quoted_owner = quote_ident(owner);
        let alter_owner_sql = format!(
            "ALTER TABLE {}.{} OWNER TO {}",
            quoted_schema, quoted_table, quoted_owner
        );
        sqlx::query(&alter_owner_sql).execute(&mut *tx).await?;

        // Add assets and datetime to table for raster/pointcloud collections
        if collection_type == "raster" || collection_type == "pointcloud" {
            let add_datetime_sql = format!(
                "ALTER TABLE {}.{} ADD COLUMN _datetime TIMESTAMPTZ",
                quoted_schema, quoted_table
            );
            sqlx::query(&add_datetime_sql).execute(&mut *tx).await?;

            let create_assets_sql = create_assets_table_sql(schema_name, &table_name);
            sqlx::query(&create_assets_sql).execute(&mut *tx).await?;
        }

        // Create import job inside the same transaction if requested
        if let Some((job_username, process_id, inputs)) = import_job {
            let job_id = Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO spatialvault.processes_jobs
                (id, process_id, owner, inputs)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(job_id)
            .bind(process_id)
            .bind(job_username)
            .bind(inputs)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(collection)
    }

    pub async fn update_collection(
        &self,
        username: &str,
        collection_id: &str,
        matches: &impl VersionMatch,
        title: Option<&str>,
        description: Option<&str>,
        new_name: Option<&str>,
        add_columns: Option<&[ColumnDef]>,
        remove_columns: Option<&[String]>,
    ) -> AppResult<Collection> {
        let mut tx = self.db.pool().begin().await?;

        // Get current collection with version check
        let current: Option<Collection> = sqlx::query_as(
            "SELECT * FROM spatialvault.collections WHERE canonical_name = $1 FOR UPDATE",
        )
        .bind(collection_id)
        .fetch_optional(&mut *tx)
        .await?;

        let current = match current {
            Some(c) => c,
            None => {
                self.check_alias(collection_id).await?;
                return Err(NotFound {
                    message: format!("Collection not found: {}", collection_id),
                }
                .build());
            }
        };

        // Check ownership (only owner can update)
        if current.owner != username {
            let has_select = Self::has_select_privilege(&mut *tx, username, &current).await?;
            self.check_ownership(username, &current, has_select)?;
        }

        // Check version if If-Match header was provided
        if !matches.matches(current.version) {
            return Err(PreconditionFailed {
                message: "Collection has been modified".to_string(),
            }
            .build());
        }

        // Determine final canonical name
        let final_name = new_name.unwrap_or(collection_id);

        // Derive new table_name from the new canonical name if renaming
        let new_table_name = if let Some(name) = new_name {
            let (_, tbl) = parse_canonical_name(name)?;
            Some(tbl)
        } else {
            None
        };

        // Update collection first (must happen before alias insert due to FK constraint)
        let collection: Collection = sqlx::query_as(
            r#"
            UPDATE spatialvault.collections
            SET
                canonical_name = $1,
                table_name = COALESCE($2, table_name),
                title = COALESCE($3, title),
                description = COALESCE($4, description),
                version = version + 1,
                updated_at = NOW()
            WHERE id = $5
            RETURNING *
            "#,
        )
        .bind(final_name)
        .bind(new_table_name.as_deref())
        .bind(title)
        .bind(description)
        .bind(current.id)
        .fetch_one(&mut *tx)
        .await?;

        // Create alias from old name after the canonical_name has been updated
        if let Some(ref new_tbl) = new_table_name {
            sqlx::query(
                "INSERT INTO spatialvault.collection_aliases (old_name, new_name) VALUES ($1, $2)",
            )
            .bind(collection_id)
            .bind(final_name)
            .execute(&mut *tx)
            .await?;

            // Rename the underlying PostgreSQL tables to match the new name
            let quoted_schema = quote_ident(&current.schema_name);
            let quoted_old_table = quote_ident(&current.table_name);
            let quoted_new_table = quote_ident(new_tbl);

            sqlx::query(&format!(
                "ALTER TABLE {}.{} RENAME TO {}",
                quoted_schema, quoted_old_table, quoted_new_table
            ))
            .execute(&mut *tx)
            .await?;

            // Rename the assets table too (only created for raster/pointcloud collections)
            if current.collection_type == "raster" || current.collection_type == "pointcloud" {
                let quoted_old_assets =
                    quote_ident(&format!("_{}_assets", current.table_name));
                let quoted_new_assets = quote_ident(&format!("_{}_assets", new_tbl));

                sqlx::query(&format!(
                    "ALTER TABLE {}.{} RENAME TO {}",
                    quoted_schema, quoted_old_assets, quoted_new_assets
                ))
                .execute(&mut *tx)
                .await?;
            }
        }

        // Handle column additions
        if let Some(cols) = add_columns {
            let quoted_schema = quote_ident(&current.schema_name);
            let effective_table = new_table_name.as_deref().unwrap_or(&current.table_name);
            let quoted_table = quote_ident(effective_table);
            let existing = self.get_user_columns(&mut *tx, &current).await?;

            for col in cols {
                validate_column_name(&col.name)?;

                // Check if column already exists
                if let Some(existing_col) = existing.iter().find(|c| c.name == col.name) {
                    let expected_type = pg_type(&col.column_type).to_lowercase();
                    if existing_col.pg_type.to_lowercase() != expected_type {
                        return Err(BadRequest {
                            message: format!(
                                "Column '{}' already exists with type '{}', cannot change to '{}'",
                                col.name, existing_col.pg_type, expected_type
                            ),
                        }
                        .build());
                    }
                    continue; // Column already exists with same type, skip
                }

                let col_type = pg_type(&col.column_type);
                let nullable = if col.nullable { "" } else { " NOT NULL" };
                let default = if let Some(ref def) = col.default {
                    format!(" DEFAULT {}", default_to_sql(def, &col.column_type)?)
                } else {
                    String::new()
                };

                let alter_sql = format!(
                    "ALTER TABLE {}.{} ADD COLUMN {} {}{}{}",
                    quoted_schema,
                    quoted_table,
                    quote_ident(&col.name),
                    col_type,
                    nullable,
                    default
                );
                sqlx::query(&alter_sql).execute(&mut *tx).await?;
            }
        }

        // Handle column removals
        if let Some(cols) = remove_columns {
            let quoted_schema = quote_ident(&current.schema_name);
            let effective_table = new_table_name.as_deref().unwrap_or(&current.table_name);
            let quoted_table = quote_ident(effective_table);

            for col_name in cols {
                validate_column_name(col_name)?;
                let alter_sql = format!(
                    "ALTER TABLE {}.{} DROP COLUMN IF EXISTS {}",
                    quoted_schema,
                    quoted_table,
                    quote_ident(col_name)
                );
                sqlx::query(&alter_sql).execute(&mut *tx).await?;
            }
        }

        tx.commit().await?;

        Ok(collection)
    }

    /// Replace a collection (PUT semantics - full replacement of mutable fields)
    pub async fn replace_collection(
        &self,
        username: &str,
        collection_id: &str,
        matches: &impl VersionMatch,
        title: &str,
        description: Option<&str>,
        columns: Option<&[ColumnDef]>,
    ) -> AppResult<Collection> {
        let mut tx = self.db.pool().begin().await?;

        // Get current collection with version check
        let current: Option<Collection> = sqlx::query_as(
            "SELECT * FROM spatialvault.collections WHERE canonical_name = $1 FOR UPDATE",
        )
        .bind(collection_id)
        .fetch_optional(&mut *tx)
        .await?;

        let current = match current {
            Some(c) => c,
            None => {
                self.check_alias(collection_id).await?;
                return Err(NotFound {
                    message: format!("Collection not found: {}", collection_id),
                }
                .build());
            }
        };

        // Check ownership first (before version check for proper error ordering)
        if current.owner != username {
            let has_select = Self::has_select_privilege(&mut *tx, username, &current).await?;
            self.check_ownership(username, &current, has_select)?;
        }

        // Check version if If-Match header was provided
        if !matches.matches(current.version) {
            return Err(PreconditionFailed {
                message: "Collection has been modified".to_string(),
            }
            .build());
        }

        // Replace collection (title and description are the only mutable fields)
        let collection: Collection = sqlx::query_as(
            r#"
            UPDATE spatialvault.collections
            SET
                title = $1,
                description = $2,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $3
            RETURNING *
            "#,
        )
        .bind(title)
        .bind(description)
        .bind(current.id)
        .fetch_one(&mut *tx)
        .await?;

        // Sync columns if provided (PUT semantics: add new, drop missing, error on type change)
        if let Some(cols) = columns {
            let quoted_schema = quote_ident(&current.schema_name);
            let quoted_table = quote_ident(&current.table_name);
            let existing = self.get_user_columns(&mut *tx, &current).await?;

            // Validate all new column names first
            for col in cols {
                validate_column_name(&col.name)?;
            }

            // Add new columns or verify existing types match
            for col in cols {
                if let Some(existing_col) = existing.iter().find(|c| c.name == col.name) {
                    let expected_type = pg_type(&col.column_type).to_lowercase();
                    if existing_col.pg_type.to_lowercase() != expected_type {
                        return Err(BadRequest {
                            message: format!(
                                "Column '{}' has type '{}', cannot change to '{}'",
                                col.name, existing_col.pg_type, expected_type
                            ),
                        }
                        .build());
                    }
                } else {
                    let col_type = pg_type(&col.column_type);
                    let nullable = if col.nullable { "" } else { " NOT NULL" };
                    let default = if let Some(ref def) = col.default {
                        format!(" DEFAULT {}", default_to_sql(def, &col.column_type)?)
                    } else {
                        String::new()
                    };
                    let alter_sql = format!(
                        "ALTER TABLE {}.{} ADD COLUMN {} {}{}{}",
                        quoted_schema,
                        quoted_table,
                        quote_ident(&col.name),
                        col_type,
                        nullable,
                        default
                    );
                    sqlx::query(&alter_sql).execute(&mut *tx).await?;
                }
            }

            // Drop columns not in the new definition
            let new_names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
            for existing_col in &existing {
                if !new_names.contains(&existing_col.name.as_str()) {
                    let alter_sql = format!(
                        "ALTER TABLE {}.{} DROP COLUMN {}",
                        quoted_schema,
                        quoted_table,
                        quote_ident(&existing_col.name)
                    );
                    sqlx::query(&alter_sql).execute(&mut *tx).await?;
                }
            }
        }

        tx.commit().await?;

        Ok(collection)
    }

    pub async fn delete_collection(
        &self,
        username: &str,
        collection_id: &str,
        matches: &impl VersionMatch,
    ) -> AppResult<()> {
        let mut tx = self.db.pool().begin().await?;

        let collection: Option<Collection> = sqlx::query_as(
            "SELECT * FROM spatialvault.collections WHERE canonical_name = $1 FOR UPDATE",
        )
        .bind(collection_id)
        .fetch_optional(&mut *tx)
        .await?;

        let collection = match collection {
            Some(c) => c,
            None => {
                self.check_alias(collection_id).await?;
                return Err(NotFound {
                    message: format!("Collection not found: {}", collection_id),
                }
                .build());
            }
        };

        // Check ownership (only owner can delete)
        if collection.owner != username {
            let has_select = Self::has_select_privilege(&mut *tx, username, &collection).await?;
            self.check_ownership(username, &collection, has_select)?;
        }

        // Check version if If-Match header was provided
        if !matches.matches(collection.version) {
            return Err(PreconditionFailed {
                message: "Collection has been modified".to_string(),
            }
            .build());
        }

        let drop_sql = format!(
            r#"DROP TABLE IF EXISTS {}.{} CASCADE"#,
            quote_ident(&collection.schema_name),
            quote_ident(&collection.table_name)
        );
        sqlx::query(&drop_sql).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM spatialvault.collections WHERE id = $1")
            .bind(collection.id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn compute_extent(&self, collection: &Collection) -> AppResult<Option<Extent>> {
        let spatial = self.compute_spatial_extent(collection).await?;
        let temporal = self.compute_temporal_extent(collection).await?;

        if spatial.is_none() && temporal.is_none() {
            return Ok(None);
        }

        Ok(Some(Extent { spatial, temporal }))
    }

    pub async fn has_datetime(&self, collection: &Collection) -> AppResult<bool> {
        let value: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = $1 AND table_name = $2 AND column_name = '_datetime' AND data_type IN ('timestamp with time zone', 'timestamp without time zone')
            )
            "#,
        )        .bind(&collection.schema_name)
            .bind(&collection.table_name)
            .fetch_one(self.db.pool())
            .await?;
        Ok(value.unwrap_or(false))
    }
    pub async fn has_assets(&self, collection: &Collection) -> AppResult<bool> {
        let assets_table = format!("_{}_assets", collection.table_name);
        let value: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                    ON tc.constraint_name = kcu.constraint_name
                   AND tc.table_schema = kcu.table_schema
                JOIN information_schema.constraint_column_usage ccu
                    ON ccu.constraint_name = tc.constraint_name
                   AND ccu.table_schema = tc.table_schema
                WHERE tc.constraint_type = 'FOREIGN KEY'
                  AND tc.table_schema = $1
                  AND tc.table_name = $2
                  AND kcu.column_name = 'item_id'
                  AND ccu.table_schema = $1
                  AND ccu.table_name = $3
                  AND ccu.column_name = '_id'
            )
            "#,
        )
        .bind(&collection.schema_name)
        .bind(&assets_table)
        .bind(&collection.table_name)
        .fetch_one(self.db.pool())
        .await?;
        Ok(value.unwrap_or(false))
    }

    /// Create the assets table for a collection if it doesn't already exist.
    /// Must be called on a service-role connection (before SET LOCAL ROLE)
    /// so DDL runs with sufficient privileges.
    pub async fn ensure_assets_table(
        &self,
        conn: &mut sqlx::PgConnection,
        collection: &Collection,
    ) -> AppResult<()> {
        let sql = create_assets_table_sql(&collection.schema_name, &collection.table_name);
        sqlx::query(&sql).execute(&mut *conn).await?;

        let alter_owner_sql = format!(
            "ALTER TABLE {}.{} OWNER TO {}",
            quote_ident(&collection.schema_name),
            quote_ident(&format!("_{}_assets", collection.table_name)),
            quote_ident(&collection.owner),
        );
        sqlx::query(&alter_owner_sql).execute(&mut *conn).await?;

        Ok(())
    }

    pub async fn get_collection_extent(
        &self,
        collection: &Collection,
        srid: i32,
    ) -> AppResult<Option<Bbox>> {
        let sql = format!(
            r#"
                    SELECT
                        ST_XMin(extent) as minx,
                        ST_YMin(extent) as miny,
                        ST_XMax(extent) as maxx,
                        ST_YMax(extent) as maxy
                    FROM (
                        SELECT ST_Extent(ST_Transform(geometry, $1)) as extent
                        FROM {}.{}
                    ) sub
                    "#,
            quote_ident(&collection.schema_name),
            quote_ident(&collection.table_name)
        );
        let result: Option<(Option<f64>, Option<f64>, Option<f64>, Option<f64>)> =
            sqlx::query_as(&sql)
                .bind(srid)
                .fetch_optional(self.db.pool())
                .await?;

        match result {
            Some((Some(minx), Some(miny), Some(maxx), Some(maxy))) => {
                Ok(Some(Bbox::two_d(minx, miny, maxx, maxy)))
            }
            _ => Ok(None),
        }
    }

    async fn compute_spatial_extent(
        &self,
        collection: &Collection,
    ) -> AppResult<Option<SpatialExtent>> {
        match self.get_collection_extent(collection, 4326).await? {
            Some(bbox) => Ok(Some(SpatialExtent {
                bbox: vec![bbox],
                crs: Some("http://www.opengis.net/def/crs/OGC/1.3/CRS84".to_string()),
            })),
            _ => Ok(None),
        }
    }

    async fn compute_temporal_extent(
        &self,
        collection: &Collection,
    ) -> AppResult<Option<TemporalExtent>> {
        let result: Option<(
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
        )> = if self.has_datetime(collection).await? {
            let sql = format!(
                r#"
                        SELECT MIN(_datetime) as min_dt, MAX(_datetime) as max_dt
                        FROM {}.{}
                        WHERE _datetime IS NOT NULL
                    "#,
                quote_ident(&collection.schema_name),
                quote_ident(&collection.table_name)
            );
            sqlx::query_as(&sql).fetch_optional(self.db.pool()).await?
        } else {
            None
        };

        match result {
            Some((min_dt, max_dt)) if min_dt.is_some() || max_dt.is_some() => {
                Ok(Some(TemporalExtent {
                    interval: vec![[
                        min_dt.map(|d| d.to_rfc3339()),
                        max_dt.map(|d| d.to_rfc3339()),
                    ]],
                }))
            }
            _ => Ok(None),
        }
    }

    pub async fn get_storage_crs(&self, collection: &Collection) -> AppResult<Option<i32>> {
        // Get SRID from geometry column
        let sql = format!(
            r#"
            SELECT ST_SRID(geometry) as srid
            FROM {}.{}
            LIMIT 1
            "#,
            quote_ident(&collection.schema_name),
            quote_ident(&collection.table_name)
        );

        let result: Option<(i32,)> = sqlx::query_as(&sql).fetch_optional(self.db.pool()).await?;

        Ok(result.map(|(srid,)| srid))
    }

    pub async fn get_collection_schema(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<CollectionSchema> {
        let collection = self.get_collection(username, collection_id).await?;

        // Get column information from PostgreSQL
        let columns: Vec<(String, String, String, Option<i32>)> = sqlx::query_as(
            r#"
            SELECT
                c.column_name,
                c.data_type,
                c.is_nullable,
                CASE WHEN c.data_type = 'USER-DEFINED' THEN
                    (SELECT srid FROM geometry_columns
                     WHERE f_table_schema = $1 AND f_table_name = $2 AND f_geometry_column = c.column_name)
                ELSE NULL END as srid
            FROM information_schema.columns c
            WHERE c.table_schema = $1 AND c.table_name = $2
            ORDER BY c.ordinal_position
            "#,
        )
        .bind(&collection.schema_name)
        .bind(&collection.table_name)
        .fetch_all(self.db.pool())
        .await?;

        // System columns to exclude from the user-facing schema
        let hidden_columns = ["_id", "_version", "_created_at", "_updated_at"];

        // Build JSON Schema properties
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (column_name, data_type, is_nullable, srid) in columns {
            // Skip system columns
            if hidden_columns.contains(&column_name.as_str()) {
                continue;
            }

            let column_schema = match data_type.as_str() {
                "uuid" => serde_json::json!({ "type": "string", "format": "uuid" }),
                "text" | "character varying" => serde_json::json!({ "type": "string" }),
                "integer" | "bigint" | "smallint" => serde_json::json!({ "type": "integer" }),
                "real" | "double precision" | "numeric" => serde_json::json!({ "type": "number" }),
                "boolean" => serde_json::json!({ "type": "boolean" }),
                "timestamp with time zone" | "timestamp without time zone" => {
                    serde_json::json!({ "type": "string", "format": "date-time" })
                }
                "date" => serde_json::json!({ "type": "string", "format": "date" }),
                "jsonb" | "json" => serde_json::json!({ "type": "object" }),
                "USER-DEFINED" => {
                    // This is likely a geometry column
                    let mut geom_schema = serde_json::json!({
                        "type": "object",
                        "description": "GeoJSON geometry"
                    });
                    if let Some(s) = srid {
                        geom_schema["x-srid"] = serde_json::json!(s);
                    }
                    geom_schema
                }
                "ARRAY" => serde_json::json!({ "type": "array" }),
                _ => serde_json::json!({ "type": "string" }),
            };

            properties.insert(column_name.clone(), column_schema);

            if is_nullable == "NO" {
                required.push(column_name);
            }
        }

        let schema = CollectionSchema {
            schema: "https://json-schema.org/draft/2020-12/schema".to_string(),
            id: format!("/collections/{}/schema", collection_id),
            schema_type: "object".to_string(),
            title: collection.title.clone(),
            properties: serde_json::Value::Object(properties),
            required: if required.is_empty() {
                None
            } else {
                Some(required)
            },
        };

        Ok(schema)
    }

    pub async fn get_collection_queryables(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<CollectionSchema> {
        let collection = self.get_collection(username, collection_id).await?;

        // Get column information from PostgreSQL, including nullability and column comments
        let columns: Vec<(String, String, String, Option<i32>, Option<String>)> = sqlx::query_as(
            r#"
            SELECT
                c.column_name,
                c.data_type,
                c.is_nullable,
                CASE WHEN c.data_type = 'USER-DEFINED' THEN
                    (SELECT srid FROM geometry_columns
                     WHERE f_table_schema = $1 AND f_table_name = $2 AND f_geometry_column = c.column_name)
                ELSE NULL END as srid,
                col_description(
                    (quote_ident($1) || '.' || quote_ident($2))::regclass,
                    c.ordinal_position
                ) as comment
            FROM information_schema.columns c
            WHERE c.table_schema = $1 AND c.table_name = $2
            ORDER BY c.ordinal_position
            "#,
        )
        .bind(&collection.schema_name)
        .bind(&collection.table_name)
        .fetch_all(self.db.pool())
        .await?;

        // Build JSON Schema properties for queryables
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        // Always include feature id as a queryable
        properties.insert(
            "id".to_string(),
            serde_json::json!({
                "type": "string",
                "title": "Feature Identifier",
                "x-ogc-role": "id"
            }),
        );

        // Columns to exclude from queryables output
        let excluded_columns = ["_id", "_version", "_created_at", "_updated_at"];

        for (column_name, data_type, is_nullable, srid, comment) in columns {
            if excluded_columns.contains(&column_name.as_str()) {
                continue;
            }

            // Map _datetime column name to "datetime" for the queryables document
            let key = if column_name == "_datetime" {
                "datetime".to_string()
            } else {
                column_name.clone()
            };

            let mut column_schema = if column_name == "geometry" || data_type == "USER-DEFINED" {
                // Primary geometry column
                let mut geom_schema = serde_json::json!({
                    "x-ogc-role": "primary-geometry",
                    "format": "geometry",
                    "$ref": "https://geojson.org/schema/Geometry.json"
                });
                if let Some(s) = srid {
                    geom_schema["x-ogc-srid"] = serde_json::json!(s);
                }
                geom_schema
            } else if column_name == "_datetime" {
                // Expose datetime as a standard 'datetime' queryable
                serde_json::json!({ "type": "string", "format": "date-time", "title": "Datetime" })
            } else {
                match data_type.as_str() {
                    "uuid" => serde_json::json!({ "type": "string", "format": "uuid" }),
                    "text" | "character varying" => serde_json::json!({ "type": "string" }),
                    "integer" | "bigint" | "smallint" => {
                        serde_json::json!({ "type": "integer" })
                    }
                    "real" | "double precision" | "numeric" => {
                        serde_json::json!({ "type": "number" })
                    }
                    "boolean" => serde_json::json!({ "type": "boolean" }),
                    "timestamp with time zone" | "timestamp without time zone" => {
                        serde_json::json!({ "type": "string", "format": "date-time" })
                    }
                    "date" => serde_json::json!({ "type": "string", "format": "date" }),
                    "jsonb" | "json" => serde_json::json!({ "type": "object" }),
                    "ARRAY" => serde_json::json!({ "type": "array" }),
                    _ => serde_json::json!({ "type": "string" }),
                }
            };

            // Add column comment as description if available
            if let Some(desc) = comment {
                column_schema["description"] = serde_json::json!(desc);
            }

            // Track non-nullable columns for the required list
            if is_nullable == "NO" {
                required.push(key.clone());
            }

            properties.insert(key, column_schema);
        }

        let schema = CollectionSchema {
            schema: "https://json-schema.org/draft/2020-12/schema".to_string(),
            id: format!("/collections/{}/queryables", collection_id),
            schema_type: "object".to_string(),
            title: collection.title.clone(),
            properties: serde_json::Value::Object(properties),
            required: if required.is_empty() {
                None
            } else {
                Some(required)
            },
        };

        Ok(schema)
    }

    /// Get user-defined columns (excludes system columns and geometry)
    pub async fn get_user_columns(
        &self,
        conn: &mut sqlx::PgConnection,
        collection: &Collection,
    ) -> AppResult<Vec<ColumnInfo>> {
        let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT column_name, data_type, is_nullable, column_default
            FROM information_schema.columns
            WHERE table_schema = $1 AND table_name = $2
            ORDER BY ordinal_position
            "#,
        )
        .bind(&collection.schema_name)
        .bind(&collection.table_name)
        .fetch_all(&mut *conn)
        .await?;

        Ok(rows
            .into_iter()
            .filter(|(name, data_type, _, _)| {
                !SYSTEM_COLUMNS.contains(&name.as_str()) && data_type != "USER-DEFINED"
            })
            .map(
                |(name, data_type, is_nullable, column_default)| ColumnInfo {
                    name,
                    pg_type: data_type,
                    is_nullable: is_nullable == "YES",
                    column_default,
                },
            )
            .collect())
    }

    pub async fn list_shares(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<Vec<ShareEntry>> {
        let collection = self.get_collection(username, collection_id).await?;

        // Check if user is owner (only owner can view shares)
        if collection.owner != username {
            return Err(Forbidden {
                message: "Only owner can view sharing settings".to_string(),
            }
            .build());
        }

        // Query PostgreSQL grants from information_schema
        let table_grants: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT grantee, privilege_type
            FROM information_schema.table_privileges
            WHERE table_schema = $1
              AND table_name = $2
              AND grantee != $3
              AND grantee != 'PUBLIC'
            ORDER BY grantee, privilege_type
            "#,
        )
        .bind(&collection.schema_name)
        .bind(&collection.table_name)
        .bind(&collection.owner)
        .fetch_all(self.db.pool())
        .await?;

        // Group grants by grantee
        let mut shares_map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for (grantee, privilege) in table_grants {
            shares_map.entry(grantee).or_default().push(privilege);
        }

        // Determine principal type by checking if role is a group (has members)
        let mut shares = Vec::new();
        for (principal, privileges) in shares_map {
            // Determine permission level based on privileges
            let permission = if privileges
                .iter()
                .any(|p| p == "INSERT" || p == "UPDATE" || p == "DELETE")
            {
                PermissionLevel::Write
            } else {
                PermissionLevel::Read
            };

            // Check if this is a group by looking for role memberships
            let is_group: (bool,) = sqlx::query_as(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM pg_auth_members
                    WHERE roleid = (SELECT oid FROM pg_roles WHERE rolname = $1)
                )
                "#,
            )
            .bind(&principal)
            .fetch_one(self.db.pool())
            .await?;

            let principal_type = if is_group.0 { "group" } else { "user" }.to_string();

            shares.push(ShareEntry {
                principal,
                principal_type,
                permission,
            });
        }

        Ok(shares)
    }

    pub async fn add_share(
        &self,
        username: &str,
        collection_id: &str,
        principal: &str,
        _principal_type: &str,
        permission: PermissionLevel,
    ) -> AppResult<()> {
        let collection = self.get_collection(username, collection_id).await?;

        if collection.owner != username {
            return Err(Forbidden {
                message: "Only owner can manage sharing".to_string(),
            }
            .build());
        }

        // Verify role exists (groups/users assumed to be pre-existing)
        let role_manager = RoleManager::new(self.db.pool());
        if !role_manager.role_exists(principal).await? {
            return Err(NotFound {
                message: format!("Role not found: {}", principal),
            }
            .build());
        }

        // Grant privileges
        let privileges = match permission {
            PermissionLevel::Read => vec!["SELECT"],
            PermissionLevel::Write => vec!["SELECT", "INSERT", "UPDATE", "DELETE"],
        };

        role_manager
            .grant_table_privileges(
                &collection.schema_name,
                &collection.table_name,
                principal,
                &privileges,
            )
            .await?;

        Ok(())
    }

    pub async fn remove_share(
        &self,
        username: &str,
        collection_id: &str,
        principal: &str,
    ) -> AppResult<()> {
        let collection = self.get_collection(username, collection_id).await?;

        if collection.owner != username {
            return Err(Forbidden {
                message: "Only owner can manage sharing".to_string(),
            }
            .build());
        }

        let role_manager = RoleManager::new(self.db.pool());
        role_manager
            .revoke_table_privileges(&collection.schema_name, &collection.table_name, principal)
            .await?;

        Ok(())
    }
}

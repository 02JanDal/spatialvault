use std::sync::Arc;
use uuid::Uuid;

use crate::api::collections::schemas::CollectionSchema;
use crate::api::collections::sharing::{PermissionLevel, ShareEntry};
use crate::api::common::{Bbox, Extent, SpatialExtent, TemporalExtent};
use crate::auth::{is_valid_role_name, quote_ident, RoleManager};
use crate::db::{Collection, Database};
use crate::error::{AppError, AppResult};

pub struct CollectionService {
    db: Arc<Database>,
}

impl CollectionService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub async fn list_collections(
        &self,
        username: &str,
        limit: u32,
        offset: u32,
    ) -> AppResult<Vec<Collection>> {
        // List collections accessible to this user
        // This includes owned collections and shared collections
        let collections: Vec<Collection> = sqlx::query_as(
            r#"
            SELECT c.*
            FROM spatialvault.collections c
            WHERE c.owner = $1
               OR EXISTS (
                   SELECT 1 FROM pg_catalog.has_table_privilege($1, c.schema_name || '.' || c.table_name, 'SELECT')
                   WHERE pg_catalog.has_table_privilege($1, c.schema_name || '.' || c.table_name, 'SELECT')
               )
            ORDER BY c.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(username)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(self.db.pool())
        .await?;

        Ok(collections)
    }

    pub async fn get_collection(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<Option<Collection>> {
        let collection: Option<Collection> = sqlx::query_as(
            "SELECT * FROM spatialvault.collections WHERE canonical_name = $1",
        )
        .bind(collection_id)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(collection)
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
        username: &str,
        canonical_name: &str,
        owner: &str,
        title: &str,
        description: Option<&str>,
        collection_type: &str,
        crs: i32,
    ) -> AppResult<Collection> {
        // Ensure user role exists
        let role_manager = RoleManager::new(self.db.pool());
        role_manager.ensure_user_role(owner).await?;

        // Parse canonical name to get schema and table name
        let parts: Vec<&str> = canonical_name.split(':').collect();
        let schema_name = parts.first().ok_or_else(|| {
            AppError::BadRequest("Invalid collection name".to_string())
        })?;
        let table_name = parts[1..].join("_");

        if table_name.is_empty() {
            return Err(AppError::BadRequest(
                "Collection name must have at least two segments".to_string(),
            ));
        }

        // Validate schema and table names to prevent SQL injection
        if !is_valid_role_name(schema_name) {
            return Err(AppError::BadRequest(format!(
                "Invalid schema name: {}",
                schema_name
            )));
        }
        if !is_valid_role_name(&table_name) {
            return Err(AppError::BadRequest(format!(
                "Invalid table name: {}",
                table_name
            )));
        }

        let id = Uuid::new_v4();

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

        // For vector collections, create the feature table
        if collection_type == "vector" {
            // Use quote_ident for safe identifier quoting (belt and suspenders with validation)
            let quoted_schema = quote_ident(schema_name);
            let quoted_table = quote_ident(&table_name);

            let create_table_sql = format!(
                r#"
                CREATE TABLE {}.{} (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    geometry geometry(Geometry, {}) NOT NULL,
                    properties JSONB DEFAULT '{{}}',
                    version BIGINT NOT NULL DEFAULT 1,
                    created_at TIMESTAMPTZ DEFAULT NOW(),
                    updated_at TIMESTAMPTZ DEFAULT NOW()
                )
                "#,
                quoted_schema, quoted_table, crs
            );
            sqlx::query(&create_table_sql).execute(&mut *tx).await?;

            // Create spatial index
            let create_index_sql = format!(
                r#"CREATE INDEX ON {}.{} USING GIST(geometry)"#,
                quoted_schema, quoted_table
            );
            sqlx::query(&create_index_sql).execute(&mut *tx).await?;
        }

        tx.commit().await?;

        Ok(collection)
    }

    pub async fn update_collection(
        &self,
        username: &str,
        collection_id: &str,
        expected_version: Option<i64>,
        title: Option<&str>,
        description: Option<&str>,
        new_name: Option<&str>,
    ) -> AppResult<Collection> {
        let mut tx = self.db.pool().begin().await?;

        // Get current collection with version check
        let current: Collection = sqlx::query_as(
            "SELECT * FROM spatialvault.collections WHERE canonical_name = $1 FOR UPDATE",
        )
        .bind(collection_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current.version != version {
                return Err(AppError::PreconditionFailed(
                    "Collection has been modified".to_string(),
                ));
            }
        }

        // Check ownership
        if current.owner != username {
            return Err(AppError::Forbidden(
                "Only owner can update collection".to_string(),
            ));
        }

        // Handle rename
        let final_name = if let Some(new_canonical_name) = new_name {
            // Create alias from old name
            sqlx::query(
                "INSERT INTO spatialvault.collection_aliases (old_name, new_name) VALUES ($1, $2)",
            )
            .bind(collection_id)
            .bind(new_canonical_name)
            .execute(&mut *tx)
            .await?;

            new_canonical_name
        } else {
            collection_id
        };

        // Update collection
        let collection: Collection = sqlx::query_as(
            r#"
            UPDATE spatialvault.collections
            SET
                canonical_name = $1,
                title = COALESCE($2, title),
                description = COALESCE($3, description),
                version = version + 1,
                updated_at = NOW()
            WHERE id = $4
            RETURNING *
            "#,
        )
        .bind(final_name)
        .bind(title)
        .bind(description)
        .bind(current.id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(collection)
    }

    /// Replace a collection (PUT semantics - full replacement of mutable fields)
    pub async fn replace_collection(
        &self,
        username: &str,
        collection_id: &str,
        expected_version: Option<i64>,
        title: &str,
        description: Option<&str>,
    ) -> AppResult<Collection> {
        let mut tx = self.db.pool().begin().await?;

        // Get current collection with version check
        let current: Collection = sqlx::query_as(
            "SELECT * FROM spatialvault.collections WHERE canonical_name = $1 FOR UPDATE",
        )
        .bind(collection_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current.version != version {
                return Err(AppError::PreconditionFailed(
                    "Collection has been modified".to_string(),
                ));
            }
        }

        // Check ownership
        if current.owner != username {
            return Err(AppError::Forbidden(
                "Only owner can update collection".to_string(),
            ));
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

        tx.commit().await?;

        Ok(collection)
    }

    pub async fn delete_collection(
        &self,
        username: &str,
        collection_id: &str,
        expected_version: Option<i64>,
    ) -> AppResult<()> {
        let mut tx = self.db.pool().begin().await?;

        let collection: Collection = sqlx::query_as(
            "SELECT * FROM spatialvault.collections WHERE canonical_name = $1 FOR UPDATE",
        )
        .bind(collection_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if collection.version != version {
                return Err(AppError::PreconditionFailed(
                    "Collection has been modified".to_string(),
                ));
            }
        }

        if collection.owner != username {
            return Err(AppError::Forbidden(
                "Only owner can delete collection".to_string(),
            ));
        }

        // Drop the table for vector collections
        if collection.collection_type == "vector" {
            let drop_sql = format!(
                r#"DROP TABLE IF EXISTS {}.{} CASCADE"#,
                quote_ident(&collection.schema_name),
                quote_ident(&collection.table_name)
            );
            sqlx::query(&drop_sql).execute(&mut *tx).await?;
        }

        // Delete items and assets for raster/pointcloud collections (cascades)
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

    async fn compute_spatial_extent(&self, collection: &Collection) -> AppResult<Option<SpatialExtent>> {
        let result: Option<(Option<f64>, Option<f64>, Option<f64>, Option<f64>)> = match collection.collection_type.as_str() {
            "vector" => {
                let sql = format!(
                    r#"
                    SELECT
                        ST_XMin(extent) as minx,
                        ST_YMin(extent) as miny,
                        ST_XMax(extent) as maxx,
                        ST_YMax(extent) as maxy
                    FROM (
                        SELECT ST_Extent(ST_Transform(geometry, 4326)) as extent
                        FROM {}.{}
                    ) sub
                    "#,
                    quote_ident(&collection.schema_name),
                    quote_ident(&collection.table_name)
                );
                sqlx::query_as(&sql).fetch_optional(self.db.pool()).await?
            }
            "raster" | "pointcloud" => {
                let sql = r#"
                    SELECT
                        ST_XMin(extent) as minx,
                        ST_YMin(extent) as miny,
                        ST_XMax(extent) as maxx,
                        ST_YMax(extent) as maxy
                    FROM (
                        SELECT ST_Extent(ST_Transform(geometry, 4326)) as extent
                        FROM spatialvault.items
                        WHERE collection_id = $1
                    ) sub
                "#;
                sqlx::query_as(sql)
                    .bind(collection.id)
                    .fetch_optional(self.db.pool())
                    .await?
            }
            _ => None,
        };

        match result {
            Some((Some(minx), Some(miny), Some(maxx), Some(maxy))) => Ok(Some(SpatialExtent {
                bbox: vec![Bbox::two_d(minx, miny, maxx, maxy)],
                crs: Some("http://www.opengis.net/def/crs/OGC/1.3/CRS84".to_string()),
            })),
            _ => Ok(None),
        }
    }

    async fn compute_temporal_extent(&self, collection: &Collection) -> AppResult<Option<TemporalExtent>> {
        let result: Option<(Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)> =
            match collection.collection_type.as_str() {
                "raster" | "pointcloud" => {
                    let sql = r#"
                        SELECT MIN(datetime) as min_dt, MAX(datetime) as max_dt
                        FROM spatialvault.items
                        WHERE collection_id = $1 AND datetime IS NOT NULL
                    "#;
                    sqlx::query_as(sql)
                        .bind(collection.id)
                        .fetch_optional(self.db.pool())
                        .await?
                }
                _ => None,
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
        if collection.collection_type != "vector" {
            return Ok(None);
        }

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

        let result: Option<(i32,)> = sqlx::query_as(&sql)
            .fetch_optional(self.db.pool())
            .await?;

        Ok(result.map(|(srid,)| srid))
    }

    pub async fn get_collection_schema(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<CollectionSchema> {
        let collection = self
            .get_collection(username, collection_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

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

        // Build JSON Schema properties
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (column_name, data_type, is_nullable, srid) in columns {
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
        let collection = self
            .get_collection(username, collection_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        // For raster/pointcloud collections, use items table properties
        if collection.collection_type == "raster" || collection.collection_type == "pointcloud" {
            // Items have: geometry, datetime, properties (JSONB)
            let mut properties = serde_json::Map::new();

            // Geometry is always queryable
            properties.insert(
                "geometry".to_string(),
                serde_json::json!({
                    "type": "object",
                    "description": "GeoJSON geometry"
                }),
            );

            // Datetime is queryable
            properties.insert(
                "datetime".to_string(),
                serde_json::json!({
                    "type": "string",
                    "format": "date-time"
                }),
            );

            // Properties field (JSONB) - note: individual properties within are queryable via CQL2
            // The top-level "properties" field is exposed as a generic "object" type.
            // Individual nested fields within properties can still be queried using CQL2
            // property paths (e.g., properties.name, properties.height), but the schema
            // doesn't enumerate them since JSONB structure can vary per feature.
            properties.insert(
                "properties".to_string(),
                serde_json::json!({
                    "type": "object",
                    "description": "Feature properties"
                }),
            );

            let schema = CollectionSchema {
                schema: "https://json-schema.org/draft/2020-12/schema".to_string(),
                id: format!("/collections/{}/queryables", collection_id),
                schema_type: "object".to_string(),
                title: format!("{} - Queryables", collection.title),
                properties: serde_json::Value::Object(properties),
                required: None,
            };

            return Ok(schema);
        }

        // For vector collections, introspect the table columns
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

        // Build JSON Schema properties - exclude system columns
        let mut properties = serde_json::Map::new();
        let system_columns = ["id", "version", "created_at", "updated_at"];

        for (column_name, data_type, _is_nullable, srid) in columns {
            // Skip system columns that aren't typically queryable
            if system_columns.contains(&column_name.as_str()) {
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

            properties.insert(column_name, column_schema);
        }

        let schema = CollectionSchema {
            schema: "https://json-schema.org/draft/2020-12/schema".to_string(),
            id: format!("/collections/{}/queryables", collection_id),
            schema_type: "object".to_string(),
            title: format!("{} - Queryables", collection.title),
            properties: serde_json::Value::Object(properties),
            required: None, // Queryables don't typically declare required fields
        };

        Ok(schema)
    }

    pub async fn list_shares(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<Vec<ShareEntry>> {
        let collection = self
            .get_collection(username, collection_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        // Check if user is owner (only owner can view shares)
        if collection.owner != username {
            return Err(AppError::Forbidden(
                "Only owner can view sharing settings".to_string(),
            ));
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
            let permission = if privileges.iter().any(|p| p == "INSERT" || p == "UPDATE" || p == "DELETE") {
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
        let collection = self
            .get_collection(username, collection_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        if collection.owner != username {
            return Err(AppError::Forbidden(
                "Only owner can manage sharing".to_string(),
            ));
        }

        // Verify role exists (groups/users assumed to be pre-existing)
        let role_manager = RoleManager::new(self.db.pool());
        if !role_manager.role_exists(principal).await? {
            return Err(AppError::NotFound(format!(
                "Role not found: {}",
                principal
            )));
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
        let collection = self
            .get_collection(username, collection_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        if collection.owner != username {
            return Err(AppError::Forbidden(
                "Only owner can manage sharing".to_string(),
            ));
        }

        let role_manager = RoleManager::new(self.db.pool());
        role_manager
            .revoke_table_privileges(
                &collection.schema_name,
                &collection.table_name,
                principal,
            )
            .await?;

        Ok(())
    }
}

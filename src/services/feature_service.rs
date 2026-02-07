use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::features::Feature;
use crate::api::features::crs::transform_geometry_sql;
use crate::api::features::query::Cql2Parser;
use crate::auth::quote_ident;
use crate::db::{Collection, Database};
use crate::error::{AppError, AppResult};
use crate::services::CollectionService;

pub struct FeatureService {
    db: Arc<Database>,
    collections: Arc<CollectionService>,
}

impl FeatureService {
    pub fn new(db: Arc<Database>, collections: Arc<CollectionService>) -> Self {
        Self { db, collections }
    }

    pub async fn list_features(
        &self,
        username: &str,
        collection_id: &str,
        limit: u32,
        offset: u32,
        bbox: Option<&str>,
        bbox_crs: Option<i32>,
        target_crs: Option<i32>,
        datetime: Option<&str>,
        filter: Option<&str>,
    ) -> AppResult<(Vec<Feature>, i64, i32)> {
        let collection = self
            .collections
            .get_collection(username, collection_id)
            .await?
            .as_collection();
        let storage_srid = self.get_storage_srid(&collection).await?;
        let geometry_expr = transform_geometry_sql("geometry", storage_srid, target_crs);

        let mut where_clauses = Vec::new();

        // Add bbox filter
        if let Some(bbox_str) = bbox {
            let parts: Vec<f64> = bbox_str.split(',').filter_map(|s| s.parse().ok()).collect();
            if parts.len() == 4 {
                let bbox_srid = bbox_crs.unwrap_or(storage_srid);
                let bbox_geom = format!(
                    "ST_MakeEnvelope({}, {}, {}, {}, {})",
                    parts[0], parts[1], parts[2], parts[3], bbox_srid
                );
                if bbox_srid != storage_srid {
                    where_clauses.push(format!(
                        "ST_Intersects(geometry, ST_Transform({}, {}))",
                        bbox_geom, storage_srid
                    ));
                } else {
                    where_clauses.push(format!("ST_Intersects(geometry, {})", bbox_geom));
                }
            }
        }

        // Add CQL2 filter
        if let Some(filter_expr) = filter {
            let sql_filter = Cql2Parser::parse_to_sql(filter_expr, "")?;
            where_clauses.push(sql_filter);
        }

        // Add datetime filter
        if let Some(dt) = datetime {
            if dt.contains('/') {
                let parts: Vec<&str> = dt.split('/').collect();
                if parts.len() == 2 {
                    let datetime_start = if parts[0] != ".." {
                        Some(chrono::DateTime::parse_from_rfc3339(parts[0]).map_err(|_| {
                            AppError::BadRequest(format!("Invalid datetime start: {}", parts[0]))
                        })?)
                    } else {
                        None
                    };
                    let datetime_end = if parts[1] != ".." {
                        Some(chrono::DateTime::parse_from_rfc3339(parts[1]).map_err(|_| {
                            AppError::BadRequest(format!("Invalid datetime end: {}", parts[1]))
                        })?)
                    } else {
                        None
                    };

                    if let Some(dt) = datetime_start {
                        where_clauses.push(format!("datetime >= {}", dt.to_rfc3339()));
                    }
                    if let Some(dt) = datetime_end {
                        where_clauses.push(format!("datetime <= {}", dt.to_rfc3339()));
                    }
                }
            } else {
                let datetime_exact = chrono::DateTime::parse_from_rfc3339(dt)
                    .map_err(|_| AppError::BadRequest(format!("Invalid datetime: {}", dt)))?;
                where_clauses.push(format!("datetime = {}", datetime_exact.to_rfc3339()));
            }
        }

        let where_clause = if where_clauses.is_empty() {
            "TRUE".to_string()
        } else {
            where_clauses.join(" AND ")
        };

        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        // Count query
        let count_sql = format!(
            r#"SELECT COUNT(*) FROM {}.{} WHERE {}"#,
            quoted_schema, quoted_table, where_clause
        );
        let count: i64 = sqlx::query_scalar(&count_sql)
            .fetch_one(self.db.pool())
            .await?;

        let has_datetime = self.collections.has_datetime(&collection).await?;
        let has_assets = self.collections.has_assets(&collection).await?;

        let datetime_column = if has_datetime {
            "datetime"
        } else {
            "NULL AS datetime"
        };

        // Data query
        let sql = format!(
            r#"
            SELECT
                id,
                ST_XMin(geometry) as minx,
                ST_YMin(geometry) as miny,
                ST_XMax(geometry) as maxx,
                ST_YMax(geometry) as maxy,
                ST_AsGeoJSON({geometry_expr})::jsonb as geometry,
                {},
                properties,
                version
            FROM {}.{}
            WHERE {}
            ORDER BY created_at DESC
            LIMIT {} OFFSET {}
            "#,
            datetime_column,
            quoted_schema,
            quoted_table,
            where_clause,
            limit,
            offset,
            geometry_expr = geometry_expr
        );

        let rows: Vec<(
            Uuid,
            f64,
            f64,
            f64,
            f64,
            serde_json::Value,
            Option<chrono::DateTime<chrono::Utc>>,
            serde_json::Value,
            i64,
        )> = sqlx::query_as(&sql).fetch_all(self.db.pool()).await?;

        // Get assets for all items
        let assets_map = if has_assets {
            self.get_assets_for_items(&rows.iter().map(|(id, ..)| *id).collect::<Vec<Uuid>>())
                .await?
        } else {
            HashMap::new()
        };

        let features: Vec<Feature> = rows
            .into_iter()
            .map(
                |(id, minx, miny, maxx, maxy, geometry, datetime, properties, _version)| {
                    let item_assets = if has_assets {
                        Some(
                            assets_map
                                .get(&id)
                                .cloned()
                                .unwrap_or_else(|| serde_json::json!({})),
                        )
                    } else {
                        None
                    };

                    let mut properties = properties.clone();
                    if let Some(dt) = datetime {
                        if let serde_json::Value::Object(ref mut map) = properties {
                            map.insert("datetime".to_string(), serde_json::json!(dt.to_rfc3339()));
                        }
                    }

                    Feature {
                        feature_type: "Feature".to_string(),
                        id: id.to_string(),
                        geometry,
                        properties,
                        links: None,
                        bbox: Some(vec![minx, miny, maxx, maxy]),
                        assets: item_assets,
                        collection: Some(collection_id.to_string()),
                        stac_version: if has_assets {
                            Some("1.0.0".to_string())
                        } else {
                            None
                        },
                        stac_extensions: if has_assets { Some(vec![]) } else { None },
                    }
                },
            )
            .collect();

        Ok((features, count, target_crs.unwrap_or(storage_srid)))
    }

    /// Build a JSON object from asset fields
    fn build_asset_json(
        href: &str,
        media_type: Option<&str>,
        title: Option<&str>,
        description: Option<&str>,
        roles: Option<&[String]>,
        file_size: Option<i64>,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut asset = serde_json::Map::new();
        asset.insert("href".to_string(), serde_json::json!(href));
        if let Some(mt) = media_type {
            asset.insert("type".to_string(), serde_json::json!(mt));
        }
        if let Some(t) = title {
            asset.insert("title".to_string(), serde_json::json!(t));
        }
        if let Some(d) = description {
            asset.insert("description".to_string(), serde_json::json!(d));
        }
        if let Some(r) = roles {
            asset.insert("roles".to_string(), serde_json::json!(r));
        }
        if let Some(size) = file_size {
            asset.insert("file:size".to_string(), serde_json::json!(size));
        }
        asset
    }

    /// Get assets for a list of item IDs
    async fn get_assets_for_items(
        &self,
        item_ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, serde_json::Value>> {
        if item_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let placeholders: Vec<String> = item_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect();

        let sql = format!(
            r#"
            SELECT item_id, key, href, type, title, description, roles, file_size
            FROM spatialvault.assets
            WHERE item_id IN ({})
            "#,
            placeholders.join(", ")
        );

        let mut query = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<Vec<String>>,
                Option<i64>,
            ),
        >(&sql);

        for id in item_ids {
            query = query.bind(id);
        }

        let rows = query.fetch_all(self.db.pool()).await?;

        // Group assets by item_id
        let mut assets_map: HashMap<Uuid, serde_json::Map<String, serde_json::Value>> =
            HashMap::new();

        for (item_id, key, href, media_type, title, description, roles, file_size) in rows {
            let asset = Self::build_asset_json(
                &href,
                media_type.as_deref(),
                title.as_deref(),
                description.as_deref(),
                roles.as_deref(),
                file_size,
            );

            assets_map
                .entry(item_id)
                .or_default()
                .insert(key, serde_json::Value::Object(asset));
        }

        let result: HashMap<Uuid, serde_json::Value> = assets_map
            .into_iter()
            .map(|(id, map)| (id, serde_json::Value::Object(map)))
            .collect();

        Ok(result)
    }

    /// Get assets for a single item
    async fn get_item_assets(&self, item_id: &Uuid) -> AppResult<serde_json::Value> {
        let assets_map = self.get_assets_for_items(&[*item_id]).await?;
        Ok(assets_map
            .get(item_id)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})))
    }

    pub async fn get_feature(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        target_crs: Option<i32>,
    ) -> AppResult<Option<(Feature, i64, i32)>> {
        let collection = self
            .collections
            .get_collection(username, collection_id)
            .await?
            .as_collection();
        let storage_srid = self.get_storage_srid(&collection).await?;
        let geometry_expr = transform_geometry_sql("geometry", storage_srid, target_crs);

        let sql = format!(
            r#"
            SELECT
                id,
                ST_AsGeoJSON({geometry_expr})::jsonb as geometry,
                ST_XMin(geometry) as minx,
                ST_YMin(geometry) as miny,
                ST_XMax(geometry) as maxx,
                ST_YMax(geometry) as maxy,
                datetime,
                properties,
                version
            FROM spatialvault.items
            WHERE collection_id = $1 AND id = $2
        "#
        );

        let row: Option<(
            Uuid,
            serde_json::Value,
            f64,
            f64,
            f64,
            f64,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
            i64,
        )> = sqlx::query_as(&sql)
            .bind(collection.id)
            .bind(feature_id)
            .fetch_optional(self.db.pool())
            .await?;

        let Some((id, geometry, minx, miny, maxx, maxy, datetime, properties, version)) = row
        else {
            return Ok(None);
        };

        // Get assets
        let assets_map = self.get_assets_for_items(&[id]).await?;
        let item_assets = assets_map
            .get(&id)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let mut props = properties.unwrap_or(serde_json::json!({}));
        if let Some(dt) = datetime {
            if let serde_json::Value::Object(ref mut map) = props {
                map.insert("datetime".to_string(), serde_json::json!(dt.to_rfc3339()));
            }
        }

        Ok(Some((
            Feature {
                feature_type: "Feature".to_string(),
                id: id.to_string(),
                geometry,
                properties: props,
                links: None,
                bbox: Some(vec![minx, miny, maxx, maxy]),
                assets: Some(item_assets),
                collection: Some(collection.id.to_string()),
                stac_version: Some("1.0.0".to_string()),
                stac_extensions: Some(vec![]),
            },
            version,
            target_crs.unwrap_or(storage_srid),
        )))
    }

    pub async fn create_feature(
        &self,
        username: &str,
        collection_id: &str,
        geometry: &serde_json::Value,
        properties: &serde_json::Value,
        datetime: Option<DateTime<Utc>>,
        option: Option<serde_json::Value>,
    ) -> AppResult<(Feature, i64)> {
        let collection = self
            .collections
            .get_collection(username, collection_id)
            .await?
            .as_collection();

        if collection.collection_type != "vector" {
            return Err(AppError::BadRequest(
                "Feature creation only available for vector collections. Use processes API for raster/pointcloud.".to_string(),
            ));
        }

        let storage_srid = self.get_storage_srid(&collection).await?;

        // Execute as the user to enforce PostgreSQL permissions
        let mut tx = self.db.begin_as(username).await?;

        // TODO: consider datetime and assets

        let sql = format!(
            r#"
            INSERT INTO {}.{} (geometry, properties)
            VALUES (ST_SetSRID(ST_GeomFromGeoJSON($1), {}), $2)
            RETURNING id
            "#,
            quote_ident(&collection.schema_name),
            quote_ident(&collection.table_name),
            storage_srid
        );

        let feature_id: Uuid = sqlx::query_scalar(&sql)
            .bind(geometry.to_string())
            .bind(properties)
            .fetch_one(&mut *tx)
            .await?;

        tx.commit().await?;

        let (feature, version, _) = self
            .get_feature(username, collection_id, feature_id, None)
            .await?
            .ok_or(AppError::BadRequest(
                "Could not find newly created feature".to_string(),
            ))?;
        Ok((feature, version))
    }

    pub async fn update_feature<Matches>(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        matches: Matches,
        geometry: Option<serde_json::Value>,
        properties: Option<serde_json::Value>,
        datetime: Option<DateTime<Utc>>,
        option: Option<serde_json::Value>,
    ) -> AppResult<(Feature, i64)>
    where
        Matches: FnOnce(i64) -> bool,
    {
        let collection = self
            .collections
            .get_collection(username, collection_id)
            .await?
            .as_collection();
        // Check write permission first (before version check for proper error ordering)
        self.check_write_permission(username, &collection).await?;

        let storage_srid = self.get_storage_srid(&collection).await?;
        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        // Execute as the user to enforce PostgreSQL permissions
        let mut tx = self.db.begin_as(username).await?;

        // Lock and check version
        let check_sql = format!(
            r#"SELECT version FROM {}.{} WHERE id = $1 FOR UPDATE"#,
            quoted_schema, quoted_table
        );
        let current: Option<i64> = sqlx::query_scalar(&check_sql)
            .bind(feature_id)
            .fetch_optional(&mut *tx)
            .await?;

        let current_version =
            current.ok_or_else(|| AppError::NotFound("Feature not found".to_string()))?;

        // Check version if If-Match header was provided
        if !matches(current_version) {
            return Err(AppError::PreconditionFailed(
                "Feature has been modified".to_string(),
            ));
        }

        // Build update
        let mut updates = vec!["version = version + 1", "updated_at = NOW()"];
        let mut binds: Vec<String> = Vec::new();

        if let Some(geom) = geometry {
            binds.push(geom.to_string());
            binds.push(storage_srid.to_string());
            updates.push("geometry = ST_SetSRID(ST_GeomFromGeoJSON($2), $3::integer)");
        }

        if properties.is_some() {
            // Merge properties using JSON concatenation
            updates.push("properties = COALESCE(properties, '{}'::jsonb) || $4");
        }

        // TODO: sync assets in database if present
        // TODO: handle datetime

        let update_sql = format!(
            r#"
            UPDATE {}.{}
            SET {}
            WHERE id = $1
            RETURNING id
            "#,
            quoted_schema,
            quoted_table,
            updates.join(", ")
        );

        let feature_id: Uuid = sqlx::query_scalar(&update_sql)
            .bind(feature_id)
            .fetch_one(&mut *tx)
            .await?;

        tx.commit().await?;

        let (feature, version, _) = self
            .get_feature(username, collection_id, feature_id, None)
            .await?
            .ok_or(AppError::BadRequest(
                "Could not find newly updated feature".to_string(),
            ))?;
        Ok((feature, version))
    }

    pub async fn replace_feature<Matches>(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        matches: Matches,
        geometry: serde_json::Value,
        properties: serde_json::Value,
        datetime: Option<DateTime<Utc>>,
        option: Option<serde_json::Value>,
    ) -> AppResult<(Feature, i64)>
    where
        Matches: FnOnce(i64) -> bool,
    {
        let collection = self
            .collections
            .get_collection(username, collection_id)
            .await?
            .as_collection();
        let storage_srid = self.get_storage_srid(&collection).await?;

        // Execute as the user to enforce PostgreSQL permissions
        let mut tx = self.db.begin_as(username).await?;

        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        // Check version
        let check_sql = format!(
            r#"SELECT version FROM {}.{} WHERE id = $1 FOR UPDATE"#,
            quoted_schema, quoted_table
        );
        let current: Option<i64> = sqlx::query_scalar(&check_sql)
            .bind(feature_id)
            .fetch_optional(&mut *tx)
            .await?;

        let current_version =
            current.ok_or_else(|| AppError::NotFound("Feature not found".to_string()))?;

        // Check version if If-Match header was provided
        if !matches(current_version) {
            return Err(AppError::PreconditionFailed(
                "Feature has been modified".to_string(),
            ));
        }

        // TODO: consider datetime and assets

        let sql = format!(
            r#"
            UPDATE {}.{}
            SET
                geometry = ST_SetSRID(ST_GeomFromGeoJSON($2), {}),
                properties = $3,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id
            "#,
            quoted_schema, quoted_table, storage_srid
        );

        let feature_id: Uuid = sqlx::query_scalar(&sql)
            .bind(feature_id)
            .bind(geometry.to_string())
            .bind(properties)
            .fetch_one(&mut *tx)
            .await?;

        tx.commit().await?;

        let (feature, version, _) = self
            .get_feature(username, collection_id, feature_id, None)
            .await?
            .ok_or(AppError::BadRequest(
                "Could not find newly updated feature".to_string(),
            ))?;
        Ok((feature, version))
    }

    pub async fn delete_feature<Matches>(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        matches: Matches,
    ) -> AppResult<()>
    where
        Matches: FnOnce(i64) -> bool,
    {
        let collection = self
            .collections
            .get_collection(username, collection_id)
            .await?
            .as_collection();

        // Check write permission first (before version check for proper error ordering)
        self.check_write_permission(username, &collection).await?;

        // Execute as the user to enforce PostgreSQL permissions
        let mut tx = self.db.begin_as(username).await?;

        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        // Check version
        let check_sql = format!(
            r#"SELECT version FROM {}.{} WHERE id = $1 FOR UPDATE"#,
            quoted_schema, quoted_table
        );
        let current: Option<i64> = sqlx::query_scalar(&check_sql)
            .bind(feature_id)
            .fetch_optional(&mut *tx)
            .await?;

        let current_version =
            current.ok_or_else(|| AppError::NotFound("Feature not found".to_string()))?;

        // Check version if If-Match header was provided
        if !matches(current_version) {
            return Err(AppError::PreconditionFailed(
                "Feature has been modified".to_string(),
            ));
        }

        let delete_sql = format!(
            r#"DELETE FROM {}.{} WHERE id = $1"#,
            quoted_schema, quoted_table
        );
        sqlx::query(&delete_sql)
            .bind(feature_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Check if the user has write permission on a collection
    /// Returns Ok(()) if user has write permission, Err(Forbidden) otherwise
    async fn check_write_permission(
        &self,
        username: &str,
        collection: &Collection,
    ) -> AppResult<()> {
        // Owner always has write permission
        if collection.owner == username {
            return Ok(());
        }

        // Check for INSERT privilege (implies write access)
        let has_write: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM information_schema.table_privileges tp
                WHERE tp.table_schema = $1
                  AND tp.table_name = $2
                  AND tp.grantee = $3
                  AND tp.privilege_type = 'INSERT'
            )
            "#,
        )
        .bind(&collection.schema_name)
        .bind(&collection.table_name)
        .bind(username)
        .fetch_one(self.db.pool())
        .await?;

        if has_write {
            Ok(())
        } else {
            Err(AppError::Forbidden("Write permission required".to_string()))
        }
    }

    async fn get_storage_srid(&self, collection: &Collection) -> AppResult<i32> {
        // Get SRID from geometry column definition
        let sql = r#"
            SELECT srid FROM geometry_columns
            WHERE f_table_schema = $1 AND f_table_name = $2
            "#;

        let result: Option<i32> = sqlx::query_scalar(sql)
            .bind(&collection.schema_name)
            .bind(&collection.table_name)
            .fetch_optional(self.db.pool())
            .await?;

        Ok(result.unwrap_or(4326))
    }
}

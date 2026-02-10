use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::common::{Asset, Assets, GeoJsonGeometry};
use crate::api::features::Feature;
use crate::api::features::crs::transform_geometry_sql;
use crate::api::features::query::Cql2Parser;
use crate::auth::quote_ident;
use crate::db::{Collection, Database};
use crate::error::{AppResult, BadRequest, Forbidden, NotFound, PreconditionFailed};
use crate::services::CollectionService;
use snafu::OptionExt;
use sqlx::{Postgres, QueryBuilder};

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

        let has_datetime = self.collections.has_datetime(&collection).await?;
        let has_assets = self.collections.has_assets(&collection).await?;

        let mut where_clauses = Vec::new();

        // Add bbox filter
        if let Some(bbox_str) = bbox {
            let parts: Vec<f64> = bbox_str.split(',').filter_map(|s| s.parse().ok()).collect();
            if parts.len() == 4 {
                let bbox_srid = bbox_crs.unwrap_or(storage_srid);
                where_clauses.push(WhereClause::Bbox {
                    minx: parts[0],
                    miny: parts[1],
                    maxx: parts[2],
                    maxy: parts[3],
                    bbox_srid,
                    storage_srid,
                });
            }
        }

        // Add CQL2 filter
        if let Some(filter_expr) = filter {
            let sql_filter = Cql2Parser::parse_to_sql(filter_expr, "")?;
            where_clauses.push(WhereClause::Cql2(sql_filter));
        }

        // Add datetime filter
        if let Some(dt) = datetime {
            if !has_datetime {
                return Err(BadRequest {
                    message: "This collection does not have datetime".to_string(),
                }
                .build());
            }
            if dt.contains('/') {
                let parts: Vec<&str> = dt.split('/').collect();
                if parts.len() == 2 {
                    let datetime_start = if parts[0] != ".." {
                        Some(
                            DateTime::parse_from_rfc3339(parts[0])
                                .map_err(|_| {
                                    BadRequest {
                                        message: format!("Invalid datetime start: {}", parts[0]),
                                    }
                                    .build()
                                })?
                                .with_timezone(&Utc),
                        )
                    } else {
                        None
                    };
                    let datetime_end = if parts[1] != ".." {
                        Some(
                            DateTime::parse_from_rfc3339(parts[1])
                                .map_err(|_| {
                                    BadRequest {
                                        message: format!("Invalid datetime end: {}", parts[1]),
                                    }
                                    .build()
                                })?
                                .with_timezone(&Utc),
                        )
                    } else {
                        None
                    };

                    if let Some(dt) = datetime_start {
                        where_clauses.push(WhereClause::DatetimeStart(dt));
                    }
                    if let Some(dt) = datetime_end {
                        where_clauses.push(WhereClause::DatetimeEnd(dt));
                    }
                }
            } else {
                let datetime_exact = DateTime::parse_from_rfc3339(dt)
                    .map_err(|_| {
                        BadRequest {
                            message: format!("Invalid datetime: {}", dt),
                        }
                        .build()
                    })?
                    .with_timezone(&Utc);
                where_clauses.push(WhereClause::DatetimeExact(datetime_exact));
            }
        }

        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        // Count query
        let mut count_builder = QueryBuilder::<Postgres>::new(format!(
            "SELECT COUNT(*) FROM {}.{} WHERE ",
            quoted_schema, quoted_table
        ));
        push_where_clauses(&mut count_builder, &where_clauses);
        let count: i64 = count_builder
            .build_query_scalar()
            .fetch_one(self.db.pool())
            .await?;

        let datetime_column = if has_datetime {
            "datetime"
        } else {
            "NULL AS datetime"
        };

        // Data query
        let mut data_builder = QueryBuilder::<Postgres>::new(format!(
            r#"
            SELECT
                id,
                ST_XMin(geometry) as minx,
                ST_YMin(geometry) as miny,
                ST_XMax(geometry) as maxx,
                ST_YMax(geometry) as maxy,
                ST_AsGeoJSON({geometry_expr})::jsonb as geometry,
                {datetime_column},
                properties,
                version
            FROM {quoted_schema}.{quoted_table}
            WHERE
            "#,
            geometry_expr = geometry_expr,
            datetime_column = datetime_column,
            quoted_schema = quoted_schema,
            quoted_table = quoted_table
        ));
        push_where_clauses(&mut data_builder, &where_clauses);
        data_builder
            .push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(limit as i64)
            .push(" OFFSET ")
            .push_bind(offset as i64);

        let rows: Vec<(
            Uuid,
            f64,
            f64,
            f64,
            f64,
            sqlx::types::Json<GeoJsonGeometry>,
            Option<DateTime<Utc>>,
            serde_json::Value,
            i64,
        )> = data_builder
            .build_query_as()
            .fetch_all(self.db.pool())
            .await?;

        // Get assets for all items
        let assets_map = if has_assets {
            self.get_assets_for_items(
                &collection,
                &rows.iter().map(|(id, ..)| *id).collect::<Vec<Uuid>>(),
            )
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
                                .unwrap_or_default(),
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
                        geometry: geometry.0,
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

    /// Build an Asset from database fields
    fn build_asset(
        href: &str,
        media_type: Option<&str>,
        title: Option<&str>,
        description: Option<&str>,
        roles: Option<&[String]>,
        file_size: Option<i64>,
    ) -> Asset {
        Asset {
            href: href.to_string(),
            media_type: media_type.map(|s| s.to_string()),
            title: title.map(|s| s.to_string()),
            description: description.map(|s| s.to_string()),
            roles: roles.map(|r| r.to_vec()),
            file_size,
        }
    }

    /// Get assets for a list of item IDs
    pub(crate) async fn get_assets_for_items(
        &self,
        collection: &Collection,
        item_ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, Assets>> {
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
            FROM {}.{}
            WHERE item_id IN ({})
            "#,
            quote_ident(&collection.schema_name),
            quote_ident(&format!("_{}_assets", collection.table_name)),
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
        let mut assets_map: HashMap<Uuid, Assets> = HashMap::new();

        for (item_id, key, href, media_type, title, description, roles, file_size) in rows {
            let asset = Self::build_asset(
                &href,
                media_type.as_deref(),
                title.as_deref(),
                description.as_deref(),
                roles.as_deref(),
                file_size,
            );

            assets_map.entry(item_id).or_default().insert(key, asset);
        }

        Ok(assets_map)
    }

    /// Get assets for a single item
    async fn get_item_assets(
        &self,
        collection: &Collection,
        item_id: &Uuid,
    ) -> AppResult<Assets> {
        let assets_map = self.get_assets_for_items(&collection, &[*item_id]).await?;
        Ok(assets_map
            .get(item_id)
            .cloned()
            .unwrap_or_default())
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

        let has_datetime = self.collections.has_datetime(&collection).await?;
        let has_assets = self.collections.has_assets(&collection).await?;

        let datetime_column = if has_datetime {
            "datetime"
        } else {
            "NULL AS datetime"
        };

        let sql = format!(
            r#"
            SELECT
                id,
                ST_AsGeoJSON({geometry_expr})::jsonb as geometry,
                ST_XMin(geometry) as minx,
                ST_YMin(geometry) as miny,
                ST_XMax(geometry) as maxx,
                ST_YMax(geometry) as maxy,
                {},
                properties,
                version
            FROM {}.{}
            WHERE id = $1
        "#,
            datetime_column,
            quote_ident(&collection.schema_name),
            quote_ident(&collection.table_name),
        );

        let row: Option<(
            Uuid,
            sqlx::types::Json<GeoJsonGeometry>,
            f64,
            f64,
            f64,
            f64,
            Option<DateTime<Utc>>,
            Option<serde_json::Value>,
            i64,
        )> = sqlx::query_as(&sql)
            .bind(feature_id)
            .fetch_optional(self.db.pool())
            .await?;

        let Some((id, geometry, minx, miny, maxx, maxy, datetime, properties, version)) = row
        else {
            return Ok(None);
        };

        // Get assets
        let assets = if has_assets {
            Some(self.get_item_assets(&collection, &id).await?)
        } else {
            None
        };

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
                geometry: geometry.0,
                properties: props,
                links: None,
                bbox: Some(vec![minx, miny, maxx, maxy]),
                assets,
                collection: Some(collection.id.to_string()),
                stac_version: if has_assets {
                    Some("1.0.0".to_string())
                } else {
                    None
                },
                stac_extensions: if has_assets { Some(vec![]) } else { None },
            },
            version,
            target_crs.unwrap_or(storage_srid),
        )))
    }

    pub async fn create_feature(
        &self,
        username: &str,
        collection_id: &str,
        geometry: &GeoJsonGeometry,
        properties: &serde_json::Value,
        datetime: Option<DateTime<Utc>>,
        assets: Option<Assets>,
    ) -> AppResult<(Feature, i64)> {
        let collection = self
            .collections
            .get_collection(username, collection_id)
            .await?
            .as_collection();

        if collection.collection_type != "vector" {
            return Err(BadRequest {
                message: "Feature creation only available for vector collections. Use processes API for raster/pointcloud.".to_string(),
            }.build());
        }

        let storage_srid = self.get_storage_srid(&collection).await?;

        if assets.as_ref().is_some_and(|a| !a.is_empty()) {
            self.collections.ensure_assets_table(&collection).await?;
        }

        // Execute as the user to enforce PostgreSQL permissions
        let mut tx = self.db.begin_as(username).await?;

        // TODO: consider datetime

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
            .bind(serde_json::to_string(geometry)?)
            .bind(properties)
            .fetch_one(&mut *tx)
            .await?;

        if let Some(ref assets) = assets {
            Self::insert_assets(
                &mut tx,
                &collection.schema_name,
                &collection.table_name,
                feature_id,
                assets,
            )
            .await?;
        }

        tx.commit().await?;

        let (feature, version, _) = self
            .get_feature(username, collection_id, feature_id, None)
            .await?
            .context(BadRequest {
                message: "Could not find newly created feature".to_string(),
            })?;
        Ok((feature, version))
    }

    pub async fn update_feature<Matches>(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        matches: Matches,
        geometry: Option<GeoJsonGeometry>,
        properties: Option<serde_json::Value>,
        datetime: Option<DateTime<Utc>>,
        assets: Option<Assets>,
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

        if assets.as_ref().is_some_and(|a| !a.is_empty()) {
            self.collections.ensure_assets_table(&collection).await?;
        }

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

        let current_version = current.context(NotFound {
            message: "Feature not found".to_string(),
        })?;

        // Check version if If-Match header was provided
        if !matches(current_version) {
            return Err(PreconditionFailed {
                message: "Feature has been modified".to_string(),
            }
            .build());
        }

        // Build update
        let mut update_builder = QueryBuilder::<Postgres>::new(format!(
            "UPDATE {}.{} SET ",
            quoted_schema, quoted_table
        ));
        update_builder.push("version = version + 1, updated_at = NOW()");

        if let Some(ref geom) = geometry {
            update_builder.push(", geometry = ST_SetSRID(ST_GeomFromGeoJSON(");
            update_builder.push_bind(serde_json::to_string(geom)?);
            update_builder.push("), ");
            update_builder.push_bind(storage_srid);
            update_builder.push("::integer)");
        }

        if let Some(props) = properties {
            // Merge properties using JSON concatenation
            update_builder.push(", properties = COALESCE(properties, '{}'::jsonb) || ");
            update_builder.push_bind(props);
        }

        // TODO: handle datetime

        update_builder.push(" WHERE id = ");
        update_builder.push_bind(feature_id);
        update_builder.push(" RETURNING id");

        let feature_id: Uuid = update_builder
            .build_query_scalar()
            .fetch_one(&mut *tx)
            .await?;

        if let Some(ref assets) = assets {
            Self::upsert_assets(
                &mut tx,
                &collection.schema_name,
                &collection.table_name,
                feature_id,
                assets,
            )
            .await?;
        }

        tx.commit().await?;

        let (feature, version, _) = self
            .get_feature(username, collection_id, feature_id, None)
            .await?
            .context(BadRequest {
                message: "Could not find newly updated feature".to_string(),
            })?;
        Ok((feature, version))
    }

    pub async fn replace_feature<Matches>(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        matches: Matches,
        geometry: GeoJsonGeometry,
        properties: serde_json::Value,
        datetime: Option<DateTime<Utc>>,
        assets: Option<Assets>,
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

        if assets.as_ref().is_some_and(|a| !a.is_empty()) {
            self.collections.ensure_assets_table(&collection).await?;
        }

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

        let current_version = current.context(NotFound {
            message: "Feature not found".to_string(),
        })?;

        // Check version if If-Match header was provided
        if !matches(current_version) {
            return Err(PreconditionFailed {
                message: "Feature has been modified".to_string(),
            }
            .build());
        }

        // TODO: consider datetime

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
            .bind(serde_json::to_string(&geometry)?)
            .bind(properties)
            .fetch_one(&mut *tx)
            .await?;

        if let Some(ref assets) = assets {
            let keep_keys: Vec<&str> = assets.keys().map(|k| k.as_str()).collect();
            Self::delete_stale_assets(
                &mut tx,
                &collection.schema_name,
                &collection.table_name,
                feature_id,
                &keep_keys,
            )
            .await?;
            Self::upsert_assets(
                &mut tx,
                &collection.schema_name,
                &collection.table_name,
                feature_id,
                assets,
            )
            .await?;
        }

        tx.commit().await?;

        let (feature, version, _) = self
            .get_feature(username, collection_id, feature_id, None)
            .await?
            .context(BadRequest {
                message: "Could not find newly updated feature".to_string(),
            })?;
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

        let current_version = current.context(NotFound {
            message: "Feature not found".to_string(),
        })?;

        // Check version if If-Match header was provided
        if !matches(current_version) {
            return Err(PreconditionFailed {
                message: "Feature has been modified".to_string(),
            }
            .build());
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
            Err(Forbidden {
                message: "Write permission required".to_string(),
            }
            .build())
        }
    }

    /// Insert asset rows for a feature.
    async fn insert_assets(
        tx: &mut sqlx::Transaction<'_, Postgres>,
        schema_name: &str,
        table_name: &str,
        item_id: Uuid,
        assets: &Assets,
    ) -> AppResult<()> {
        if assets.is_empty() {
            return Ok(());
        }
        let quoted_schema = quote_ident(schema_name);
        let quoted_assets_table = quote_ident(&format!("_{}_assets", table_name));
        for (key, asset) in assets {
            let sql = format!(
                r#"
                INSERT INTO {quoted_schema}.{quoted_assets_table}
                    (item_id, key, href, type, title, description, roles, file_size)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            );
            sqlx::query(&sql)
                .bind(item_id)
                .bind(key)
                .bind(&asset.href)
                .bind(asset.media_type.as_deref())
                .bind(asset.title.as_deref())
                .bind(asset.description.as_deref())
                .bind(asset.roles.as_deref())
                .bind(asset.file_size)
                .execute(&mut **tx)
                .await?;
        }
        Ok(())
    }

    /// Upsert asset rows for a feature (PATCH merge semantics).
    async fn upsert_assets(
        tx: &mut sqlx::Transaction<'_, Postgres>,
        schema_name: &str,
        table_name: &str,
        item_id: Uuid,
        assets: &Assets,
    ) -> AppResult<()> {
        if assets.is_empty() {
            return Ok(());
        }
        let quoted_schema = quote_ident(schema_name);
        let quoted_assets_table = quote_ident(&format!("_{}_assets", table_name));
        for (key, asset) in assets {
            let sql = format!(
                r#"
                INSERT INTO {quoted_schema}.{quoted_assets_table}
                    (item_id, key, href, type, title, description, roles, file_size)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (item_id, key) DO UPDATE SET
                    href = EXCLUDED.href,
                    type = EXCLUDED.type,
                    title = EXCLUDED.title,
                    description = EXCLUDED.description,
                    roles = EXCLUDED.roles,
                    file_size = EXCLUDED.file_size
                "#,
            );
            sqlx::query(&sql)
                .bind(item_id)
                .bind(key)
                .bind(&asset.href)
                .bind(asset.media_type.as_deref())
                .bind(asset.title.as_deref())
                .bind(asset.description.as_deref())
                .bind(asset.roles.as_deref())
                .bind(asset.file_size)
                .execute(&mut **tx)
                .await?;
        }
        Ok(())
    }

    /// Delete asset rows whose keys are not in `keep_keys`.
    async fn delete_stale_assets(
        tx: &mut sqlx::Transaction<'_, Postgres>,
        schema_name: &str,
        table_name: &str,
        item_id: Uuid,
        keep_keys: &[&str],
    ) -> AppResult<()> {
        let quoted_schema = quote_ident(schema_name);
        let quoted_assets_table = quote_ident(&format!("_{}_assets", table_name));
        if keep_keys.is_empty() {
            let sql = format!(
                "DELETE FROM {quoted_schema}.{quoted_assets_table} WHERE item_id = $1"
            );
            sqlx::query(&sql).bind(item_id).execute(&mut **tx).await?;
        } else {
            let mut builder = QueryBuilder::<Postgres>::new(format!(
                "DELETE FROM {quoted_schema}.{quoted_assets_table} WHERE item_id = "
            ));
            builder.push_bind(item_id);
            builder.push(" AND key NOT IN (");
            let mut separated = builder.separated(", ");
            for key in keep_keys {
                separated.push_bind(*key);
            }
            separated.push_unseparated(")");
            builder.build().execute(&mut **tx).await?;
        }
        Ok(())
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

#[derive(Debug)]
enum WhereClause {
    Bbox {
        minx: f64,
        miny: f64,
        maxx: f64,
        maxy: f64,
        bbox_srid: i32,
        storage_srid: i32,
    },
    Cql2(String),
    DatetimeStart(DateTime<Utc>),
    DatetimeEnd(DateTime<Utc>),
    DatetimeExact(DateTime<Utc>),
}

impl WhereClause {
    fn push_sql<'a>(&'a self, builder: &mut QueryBuilder<'a, Postgres>) {
        match self {
            WhereClause::Bbox {
                minx,
                miny,
                maxx,
                maxy,
                bbox_srid,
                storage_srid,
            } => {
                if bbox_srid != storage_srid {
                    builder.push("ST_Intersects(geometry, ST_Transform(ST_MakeEnvelope(");
                    builder.push_bind(minx);
                    builder.push(", ");
                    builder.push_bind(miny);
                    builder.push(", ");
                    builder.push_bind(maxx);
                    builder.push(", ");
                    builder.push_bind(maxy);
                    builder.push(", ");
                    builder.push_bind(bbox_srid);
                    builder.push("), ");
                    builder.push_bind(storage_srid);
                    builder.push("))");
                } else {
                    builder.push("ST_Intersects(geometry, ST_MakeEnvelope(");
                    builder.push_bind(minx);
                    builder.push(", ");
                    builder.push_bind(miny);
                    builder.push(", ");
                    builder.push_bind(maxx);
                    builder.push(", ");
                    builder.push_bind(maxy);
                    builder.push(", ");
                    builder.push_bind(bbox_srid);
                    builder.push("))");
                }
            }
            WhereClause::Cql2(sql) => {
                builder.push(sql);
            }
            WhereClause::DatetimeStart(dt) => {
                builder.push("datetime >= ");
                builder.push_bind(dt);
            }
            WhereClause::DatetimeEnd(dt) => {
                builder.push("datetime <= ");
                builder.push_bind(dt);
            }
            WhereClause::DatetimeExact(dt) => {
                builder.push("datetime = ");
                builder.push_bind(dt);
            }
        }
    }
}

fn push_where_clauses<'a>(builder: &mut QueryBuilder<'a, Postgres>, clauses: &'a [WhereClause]) {
    if clauses.is_empty() {
        builder.push("TRUE");
        return;
    }

    for (idx, clause) in clauses.iter().enumerate() {
        if idx > 0 {
            builder.push(" AND ");
        }
        clause.push_sql(builder);
    }
}

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::common::etag::VersionMatch;
use crate::api::common::{Asset, Assets, GeoJsonGeometry};
use crate::api::features::Feature;
use crate::api::features::crs::transform_geometry_sql;
use crate::api::features::query::Cql2Parser;
use crate::auth::quote_ident;
use crate::db::{Collection, Database};
use crate::error::{AppResult, BadRequest, Forbidden, NotFound, PreconditionFailed};
use crate::services::CollectionService;
use crate::services::collection_service::SYSTEM_COLUMNS;
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
        filter_lang: Option<&str>,
    ) -> AppResult<(Vec<Feature>, i64, i32)> {
        let cwc = self
            .collections
            .get_collection(username, collection_id)
            .await?;
        let storage_srid = cwc.storage_crs;
        let collection = cwc.as_collection();

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
            let sql_filter = if filter_lang == Some("cql2-json") {
                Cql2Parser::parse_json_to_sql(filter_expr, "")?
            } else {
                Cql2Parser::parse_to_sql(filter_expr, "")?
            };
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

        // Build the system columns exclusion list for to_jsonb
        let system_exclusions = SYSTEM_COLUMNS
            .iter()
            .map(|c| format!("- '{}'", c))
            .collect::<Vec<_>>()
            .join(" ");

        // Count query
        let mut count_builder = QueryBuilder::<Postgres>::new(format!(
            "SELECT COUNT(*) FROM {quoted_schema}.{quoted_table} t WHERE ",
        ));
        push_where_clauses(&mut count_builder, &where_clauses);
        let count: i64 = count_builder
            .build_query_scalar()
            .fetch_one(self.db.pool())
            .await?;

        let datetime_column = if has_datetime {
            "t._datetime"
        } else {
            "NULL AS datetime"
        };

        // Data query - reconstruct properties from real columns
        let mut data_builder = QueryBuilder::<Postgres>::new(format!(
            r#"
            SELECT
                t._id,
                ST_XMin(t.geometry) as minx,
                ST_YMin(t.geometry) as miny,
                ST_XMax(t.geometry) as maxx,
                ST_YMax(t.geometry) as maxy,
                ST_AsGeoJSON({geometry_expr})::jsonb as geometry,
                {datetime_column},
                (to_jsonb(t.*) {system_exclusions}) as properties,
                t._version
            FROM {quoted_schema}.{quoted_table} t
            WHERE
            "#,
            geometry_expr = transform_geometry_sql("t.geometry", storage_srid, target_crs),
        ));
        push_where_clauses(&mut data_builder, &where_clauses);
        data_builder
            .push(" ORDER BY t._created_at DESC LIMIT ")
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
                        Some(assets_map.get(&id).cloned().unwrap_or_default())
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
    async fn get_item_assets(&self, collection: &Collection, item_id: &Uuid) -> AppResult<Assets> {
        let assets_map = self.get_assets_for_items(&collection, &[*item_id]).await?;
        Ok(assets_map.get(item_id).cloned().unwrap_or_default())
    }

    /// Read a feature from the given connection (pool or transaction).
    /// Caller provides pre-computed collection metadata to avoid extra queries.
    async fn read_feature(
        conn: &mut sqlx::PgConnection,
        collection: &Collection,
        collection_id: &str,
        feature_id: Uuid,
        has_datetime: bool,
        has_assets: bool,
    ) -> AppResult<Option<(Feature, i64)>> {
        let datetime_column = if has_datetime {
            "t._datetime"
        } else {
            "NULL AS datetime"
        };

        let system_exclusions = SYSTEM_COLUMNS
            .iter()
            .map(|c| format!("- '{}'", c))
            .collect::<Vec<_>>()
            .join(" ");

        let sql = format!(
            r#"
            SELECT
                t._id,
                ST_AsGeoJSON(t.geometry)::jsonb as geometry,
                ST_XMin(t.geometry) as minx,
                ST_YMin(t.geometry) as miny,
                ST_XMax(t.geometry) as maxx,
                ST_YMax(t.geometry) as maxy,
                {datetime_column},
                (to_jsonb(t.*) {system_exclusions}) as properties,
                t._version
            FROM {quoted_schema}.{quoted_table} t
            WHERE t._id = $1
            "#,
            quoted_schema = quote_ident(&collection.schema_name),
            quoted_table = quote_ident(&collection.table_name),
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
            .fetch_optional(&mut *conn)
            .await?;

        let Some((id, geometry, minx, miny, maxx, maxy, datetime, properties, version)) = row
        else {
            return Ok(None);
        };

        // Get assets from the same connection
        let assets = if has_assets {
            let assets_sql = format!(
                r#"
                SELECT key, href, type, title, description, roles, file_size
                FROM {}.{}
                WHERE item_id = $1
                "#,
                quote_ident(&collection.schema_name),
                quote_ident(&format!("_{}_assets", collection.table_name)),
            );
            let asset_rows: Vec<(
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<Vec<String>>,
                Option<i64>,
            )> = sqlx::query_as(&assets_sql)
                .bind(id)
                .fetch_all(&mut *conn)
                .await?;

            let mut assets_map = Assets::new();
            for (key, href, media_type, title, description, roles, file_size) in asset_rows {
                assets_map.insert(
                    key,
                    Self::build_asset(
                        &href,
                        media_type.as_deref(),
                        title.as_deref(),
                        description.as_deref(),
                        roles.as_deref(),
                        file_size,
                    ),
                );
            }
            Some(assets_map)
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
                collection: Some(collection_id.to_string()),
                stac_version: if has_assets {
                    Some("1.0.0".to_string())
                } else {
                    None
                },
                stac_extensions: if has_assets { Some(vec![]) } else { None },
            },
            version,
        )))
    }

    pub async fn get_feature(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        target_crs: Option<i32>,
    ) -> AppResult<Option<(Feature, i64, i32)>> {
        let cwc = self
            .collections
            .get_collection(username, collection_id)
            .await?;
        let storage_srid = cwc.storage_crs;
        let collection = cwc.as_collection();

        let has_datetime = self.collections.has_datetime(&collection).await?;
        let has_assets = self.collections.has_assets(&collection).await?;

        let datetime_column = if has_datetime {
            "t._datetime"
        } else {
            "NULL AS datetime"
        };

        let system_exclusions = SYSTEM_COLUMNS
            .iter()
            .map(|c| format!("- '{}'", c))
            .collect::<Vec<_>>()
            .join(" ");

        let sql = format!(
            r#"
            SELECT
                t._id,
                ST_AsGeoJSON({geometry_expr})::jsonb as geometry,
                ST_XMin(t.geometry) as minx,
                ST_YMin(t.geometry) as miny,
                ST_XMax(t.geometry) as maxx,
                ST_YMax(t.geometry) as maxy,
                {datetime_column},
                (to_jsonb(t.*) {system_exclusions}) as properties,
                t._version
            FROM {quoted_schema}.{quoted_table} t
            WHERE t._id = $1
        "#,
            geometry_expr = transform_geometry_sql("t.geometry", storage_srid, target_crs),
            quoted_schema = quote_ident(&collection.schema_name),
            quoted_table = quote_ident(&collection.table_name),
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
        _datetime: Option<DateTime<Utc>>,
        assets: Option<Assets>,
    ) -> AppResult<(Feature, i64)> {
        let cwc = self
            .collections
            .get_collection(username, collection_id)
            .await?;
        let storage_srid = cwc.storage_crs;
        let collection = cwc.as_collection();

        if collection.collection_type != "vector" {
            return Err(BadRequest {
                message: "Feature creation only available for vector collections. Use processes API for raster/pointcloud.".to_string(),
            }.build());
        }
        let has_datetime = self.collections.has_datetime(&collection).await?;
        let has_assets = assets.as_ref().is_some_and(|a| !a.is_empty());

        // Start transaction as service-role for DDL, then switch to user-role
        let mut tx = self.db.pool().begin().await?;

        if has_assets {
            self.collections
                .ensure_assets_table(&mut *tx, &collection)
                .await?;
        }

        // Get user columns before switching role (information_schema query)
        let user_columns = self
            .collections
            .get_user_columns(&mut *tx, &collection)
            .await?;

        // Switch to user-role to enforce PostgreSQL permissions
        let set_role_sql = format!("SET LOCAL ROLE {}", quote_ident(username));
        sqlx::query(&set_role_sql).execute(&mut *tx).await?;

        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);
        if let serde_json::Value::Object(map) = properties {
            for key in map.keys() {
                if key == "datetime" {
                    continue; // datetime handled separately
                }
                if key == "geometry" {
                    return Err(BadRequest {
                        message: "'geometry' is not allowed as a property name".to_string(),
                    }
                    .build());
                }
                if !user_columns.iter().any(|c| c.name == *key) {
                    return Err(BadRequest {
                        message: format!(
                            "Unknown property '{}'. Define it as a column on the collection first.",
                            key
                        ),
                    }
                    .build());
                }
            }
        }

        // Strip datetime from properties for the record
        let mut props_for_record = properties.clone();
        if let serde_json::Value::Object(map) = &mut props_for_record {
            map.remove("datetime");
        }

        let feature_id: Uuid = if user_columns.is_empty() {
            let sql = format!(
                r#"
                INSERT INTO {quoted_schema}.{quoted_table} (geometry)
                VALUES (ST_SetSRID(ST_GeomFromGeoJSON($1), {storage_srid}))
                RETURNING _id
                "#,
            );
            sqlx::query_scalar(&sql)
                .bind(serde_json::to_string(geometry)?)
                .fetch_one(&mut *tx)
                .await?
        } else {
            // Use jsonb_populate_record to decompose properties into columns
            let col_names: Vec<String> =
                user_columns.iter().map(|c| quote_ident(&c.name)).collect();
            let col_refs: Vec<String> = user_columns
                .iter()
                .map(|c| format!("r.{}", quote_ident(&c.name)))
                .collect();

            let sql = format!(
                r#"
                INSERT INTO {quoted_schema}.{quoted_table} (geometry, {cols})
                SELECT ST_SetSRID(ST_GeomFromGeoJSON($1), {storage_srid}), {col_refs}
                FROM jsonb_populate_record(NULL::{quoted_schema}.{quoted_table}, $2) r
                RETURNING _id
                "#,
                cols = col_names.join(", "),
                col_refs = col_refs.join(", "),
            );
            sqlx::query_scalar(&sql)
                .bind(serde_json::to_string(geometry)?)
                .bind(&props_for_record)
                .fetch_one(&mut *tx)
                .await?
        };

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

        // Read back the feature before committing
        let (feature, version) = Self::read_feature(
            &mut *tx,
            &collection,
            collection_id,
            feature_id,
            has_datetime,
            has_assets,
        )
        .await?
        .context(BadRequest {
            message: "Could not find newly created feature".to_string(),
        })?;

        tx.commit().await?;

        Ok((feature, version))
    }

    pub async fn update_feature(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        matches: &impl VersionMatch,
        geometry: Option<GeoJsonGeometry>,
        properties: Option<serde_json::Value>,
        _datetime: Option<DateTime<Utc>>,
        assets: Option<Assets>,
    ) -> AppResult<(Feature, i64)> {
        let cwc = self
            .collections
            .get_collection(username, collection_id)
            .await?;
        let storage_srid = cwc.storage_crs;
        let collection = cwc.as_collection();
        // Check write permission first (before version check for proper error ordering)
        self.check_write_permission(username, &collection).await?;
        let has_datetime = self.collections.has_datetime(&collection).await?;
        let has_assets_input = assets.as_ref().is_some_and(|a| !a.is_empty());
        let has_assets = has_assets_input || self.collections.has_assets(&collection).await?;
        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        // Start transaction as service-role for DDL, then switch to user-role
        let mut tx = self.db.pool().begin().await?;

        if has_assets_input {
            self.collections
                .ensure_assets_table(&mut *tx, &collection)
                .await?;
        }

        // Get user columns for validation
        let user_columns = self
            .collections
            .get_user_columns(&mut *tx, &collection)
            .await?;

        // Switch to user-role to enforce PostgreSQL permissions
        let set_role_sql = format!("SET LOCAL ROLE {}", quote_ident(username));
        sqlx::query(&set_role_sql).execute(&mut *tx).await?;

        // Lock and check version
        let check_sql = format!(
            r#"SELECT _version FROM {}.{} WHERE _id = $1 FOR UPDATE"#,
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
        if !matches.matches(current_version) {
            return Err(PreconditionFailed {
                message: "Feature has been modified".to_string(),
            }
            .build());
        }

        // Validate and strip properties
        let mut props_for_record = serde_json::Value::Null;
        if let Some(props) = properties {
            if let serde_json::Value::Object(ref map) = props {
                for key in map.keys() {
                    if key == "datetime" {
                        continue;
                    }
                    if key == "geometry" {
                        return Err(BadRequest {
                            message: "'geometry' is not allowed as a property name".to_string(),
                        }
                        .build());
                    }
                    if !user_columns.iter().any(|c| c.name == *key) {
                        return Err(BadRequest {
                            message: format!("Unknown property '{}'. Define it as a column on the collection first.", key),
                        }.build());
                    }
                }
            }
            let mut stripped = props.clone();
            if let serde_json::Value::Object(ref mut map) = stripped {
                map.remove("datetime");
            }
            props_for_record = stripped;
        }

        let has_props = !props_for_record.is_null() && !user_columns.is_empty();

        // Build update SQL with per-column CASE for PATCH semantics
        if has_props {
            let col_sets: Vec<String> = user_columns
                .iter()
                .map(|c| {
                    let qn = quote_ident(&c.name);
                    format!(
                        "{qn} = CASE WHEN $2 ? '{name}' THEN r.{qn} ELSE t.{qn} END",
                        name = c.name
                    )
                })
                .collect();

            let mut sql = format!(
                "UPDATE {quoted_schema}.{quoted_table} t SET _version = t._version + 1, _updated_at = NOW()"
            );

            sql.push_str(&format!(", {}", col_sets.join(", ")));

            if geometry.is_some() {
                sql.push_str(&format!(
                    ", geometry = ST_SetSRID(ST_GeomFromGeoJSON($3), {})",
                    storage_srid
                ));
                sql.push_str(&format!(
                    " FROM jsonb_populate_record(NULL::{quoted_schema}.{quoted_table}, $2) r WHERE t._id = $1 RETURNING t._id"
                ));

                let geom_str = serde_json::to_string(geometry.as_ref().unwrap())?;
                let feature_id_result: Uuid = sqlx::query_scalar(&sql)
                    .bind(feature_id)
                    .bind(&props_for_record)
                    .bind(&geom_str)
                    .fetch_one(&mut *tx)
                    .await?;
                // feature_id is reassigned below
                let _ = feature_id_result;
            } else {
                sql.push_str(&format!(
                    " FROM jsonb_populate_record(NULL::{quoted_schema}.{quoted_table}, $2) r WHERE t._id = $1 RETURNING t._id"
                ));

                let _: Uuid = sqlx::query_scalar(&sql)
                    .bind(feature_id)
                    .bind(&props_for_record)
                    .fetch_one(&mut *tx)
                    .await?;
            }
        } else {
            // No user columns to update, just geometry and system fields
            let mut update_builder = QueryBuilder::<Postgres>::new(format!(
                "UPDATE {quoted_schema}.{quoted_table} SET "
            ));
            update_builder.push("_version = _version + 1, _updated_at = NOW()");

            if let Some(ref geom) = geometry {
                update_builder.push(", geometry = ST_SetSRID(ST_GeomFromGeoJSON(");
                update_builder.push_bind(serde_json::to_string(geom)?);
                update_builder.push("), ");
                update_builder.push_bind(storage_srid);
                update_builder.push("::integer)");
            }

            update_builder.push(" WHERE _id = ");
            update_builder.push_bind(feature_id);
            update_builder.push(" RETURNING _id");

            let _: Uuid = update_builder
                .build_query_scalar()
                .fetch_one(&mut *tx)
                .await?;
        }

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

        // Read back the feature before committing
        let (feature, version) = Self::read_feature(
            &mut *tx,
            &collection,
            collection_id,
            feature_id,
            has_datetime,
            has_assets,
        )
        .await?
        .context(BadRequest {
            message: "Could not find newly updated feature".to_string(),
        })?;

        tx.commit().await?;

        Ok((feature, version))
    }

    pub async fn replace_feature(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        matches: &impl VersionMatch,
        geometry: GeoJsonGeometry,
        properties: serde_json::Value,
        _datetime: Option<DateTime<Utc>>,
        assets: Option<Assets>,
    ) -> AppResult<(Feature, i64)> {
        let cwc = self
            .collections
            .get_collection(username, collection_id)
            .await?;
        let storage_srid = cwc.storage_crs;
        let collection = cwc.as_collection();
        let has_datetime = self.collections.has_datetime(&collection).await?;
        let has_assets_input = assets.as_ref().is_some_and(|a| !a.is_empty());
        let has_assets = has_assets_input || self.collections.has_assets(&collection).await?;

        // Start transaction as service-role for DDL, then switch to user-role
        let mut tx = self.db.pool().begin().await?;

        if has_assets_input {
            self.collections
                .ensure_assets_table(&mut *tx, &collection)
                .await?;
        }

        // Get user columns and validate properties
        let user_columns = self
            .collections
            .get_user_columns(&mut *tx, &collection)
            .await?;

        // Switch to user-role to enforce PostgreSQL permissions
        let set_role_sql = format!("SET LOCAL ROLE {}", quote_ident(username));
        sqlx::query(&set_role_sql).execute(&mut *tx).await?;

        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        // Check version
        let check_sql = format!(
            r#"SELECT _version FROM {}.{} WHERE _id = $1 FOR UPDATE"#,
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
        if !matches.matches(current_version) {
            return Err(PreconditionFailed {
                message: "Feature has been modified".to_string(),
            }
            .build());
        }

        // Validate properties
        if let serde_json::Value::Object(ref map) = properties {
            for key in map.keys() {
                if key == "datetime" {
                    continue;
                }
                if key == "geometry" {
                    return Err(BadRequest {
                        message: "'geometry' is not allowed as a property name".to_string(),
                    }
                    .build());
                }
                if !user_columns.iter().any(|c| c.name == *key) {
                    return Err(BadRequest {
                        message: format!(
                            "Unknown property '{}'. Define it as a column on the collection first.",
                            key
                        ),
                    }
                    .build());
                }
            }
        }

        let mut props_for_record = properties.clone();
        if let serde_json::Value::Object(ref mut map) = props_for_record {
            map.remove("datetime");
        }

        if user_columns.is_empty() {
            let sql = format!(
                r#"
                UPDATE {quoted_schema}.{quoted_table}
                SET
                    geometry = ST_SetSRID(ST_GeomFromGeoJSON($2), {storage_srid}),
                    _version = _version + 1,
                    _updated_at = NOW()
                WHERE _id = $1
                RETURNING _id
                "#,
            );
            let _: Uuid = sqlx::query_scalar(&sql)
                .bind(feature_id)
                .bind(serde_json::to_string(&geometry)?)
                .fetch_one(&mut *tx)
                .await?;
        } else {
            // PUT semantics: all user columns get the value from the record (NULL if missing)
            let col_sets: Vec<String> = user_columns
                .iter()
                .map(|c| {
                    let qn = quote_ident(&c.name);
                    format!("{qn} = r.{qn}")
                })
                .collect();

            let sql = format!(
                r#"
                UPDATE {quoted_schema}.{quoted_table} t
                SET
                    geometry = ST_SetSRID(ST_GeomFromGeoJSON($2), {storage_srid}),
                    _version = t._version + 1,
                    _updated_at = NOW(),
                    {col_sets}
                FROM jsonb_populate_record(NULL::{quoted_schema}.{quoted_table}, $3) r
                WHERE t._id = $1
                RETURNING t._id
                "#,
                col_sets = col_sets.join(", "),
            );
            let _: Uuid = sqlx::query_scalar(&sql)
                .bind(feature_id)
                .bind(serde_json::to_string(&geometry)?)
                .bind(&props_for_record)
                .fetch_one(&mut *tx)
                .await?;
        }

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

        // Read back the feature before committing
        let (feature, version) = Self::read_feature(
            &mut *tx,
            &collection,
            collection_id,
            feature_id,
            has_datetime,
            has_assets,
        )
        .await?
        .context(BadRequest {
            message: "Could not find newly updated feature".to_string(),
        })?;

        tx.commit().await?;

        Ok((feature, version))
    }

    pub async fn delete_feature(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        matches: &impl VersionMatch,
    ) -> AppResult<()> {
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
            r#"SELECT _version FROM {}.{} WHERE _id = $1 FOR UPDATE"#,
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
        if !matches.matches(current_version) {
            return Err(PreconditionFailed {
                message: "Feature has been modified".to_string(),
            }
            .build());
        }

        let delete_sql = format!(
            r#"DELETE FROM {}.{} WHERE _id = $1"#,
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
            let sql =
                format!("DELETE FROM {quoted_schema}.{quoted_assets_table} WHERE item_id = $1");
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
                builder.push("_datetime >= ");
                builder.push_bind(dt);
            }
            WhereClause::DatetimeEnd(dt) => {
                builder.push("_datetime <= ");
                builder.push_bind(dt);
            }
            WhereClause::DatetimeExact(dt) => {
                builder.push("_datetime = ");
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

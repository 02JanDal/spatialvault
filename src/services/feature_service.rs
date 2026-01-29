use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::features::crs::transform_geometry_sql;
use crate::api::features::query::Cql2Parser;
use crate::api::features::Feature;
use crate::auth::quote_ident;
use crate::db::{Collection, Database};
use crate::error::{AppError, AppResult};

pub struct FeatureService {
    db: Arc<Database>,
}

impl FeatureService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
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
    ) -> AppResult<(Vec<Feature>, usize, i32)> {
        let collection = self.get_collection(collection_id).await?;

        match collection.collection_type.as_str() {
            "vector" => {
                self.list_vector_features(
                    &collection,
                    limit,
                    offset,
                    bbox,
                    bbox_crs,
                    target_crs,
                    filter,
                )
                .await
            }
            "raster" | "pointcloud" => {
                self.list_items(
                    &collection,
                    collection_id,
                    limit,
                    offset,
                    bbox,
                    datetime,
                )
                .await
            }
            _ => Err(AppError::BadRequest(format!(
                "Unknown collection type: {}",
                collection.collection_type
            ))),
        }
    }

    /// List vector features from user-schema tables
    async fn list_vector_features(
        &self,
        collection: &Collection,
        limit: u32,
        offset: u32,
        bbox: Option<&str>,
        bbox_crs: Option<i32>,
        target_crs: Option<i32>,
        filter: Option<&str>,
    ) -> AppResult<(Vec<Feature>, usize, i32)> {
        let storage_srid = self.get_storage_srid(collection).await?;
        let geometry_expr = transform_geometry_sql("geometry", storage_srid, target_crs);

        let mut where_clauses = Vec::new();

        // Add bbox filter
        if let Some(bbox_str) = bbox {
            let parts: Vec<f64> = bbox_str
                .split(',')
                .filter_map(|s| s.parse().ok())
                .collect();
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
        let count: (i64,) = sqlx::query_as(&count_sql)
            .fetch_one(self.db.pool())
            .await?;

        // Data query
        let sql = format!(
            r#"
            SELECT
                id::text,
                ST_AsGeoJSON({geometry_expr})::jsonb as geometry,
                properties,
                version
            FROM {}.{}
            WHERE {}
            ORDER BY created_at DESC
            LIMIT {} OFFSET {}
            "#,
            quoted_schema, quoted_table, where_clause, limit, offset,
            geometry_expr = geometry_expr
        );

        let rows: Vec<(String, serde_json::Value, Option<serde_json::Value>, i64)> =
            sqlx::query_as(&sql).fetch_all(self.db.pool()).await?;

        let features: Vec<Feature> = rows
            .into_iter()
            .map(|(id, geometry, properties, _version)| Feature {
                feature_type: "Feature".to_string(),
                id,
                geometry,
                properties: properties.unwrap_or(serde_json::json!({})),
                links: None,
                bbox: None,
                assets: None,
                collection: None,
                stac_version: None,
                stac_extensions: None,
            })
            .collect();

        Ok((features, count.0 as usize, target_crs.unwrap_or(storage_srid)))
    }

    /// List raster/pointcloud items from spatialvault.items (with assets)
    async fn list_items(
        &self,
        collection: &Collection,
        collection_id: &str,
        limit: u32,
        offset: u32,
        bbox: Option<&str>,
        datetime: Option<&str>,
    ) -> AppResult<(Vec<Feature>, usize, i32)> {
        // Build parameterized query with dynamic conditions
        let mut where_clauses = vec!["collection_id = $1".to_string()];
        let mut param_index = 2u32;

        // Parse bbox for parameterized query
        let bbox_coords: Option<[f64; 4]> = bbox.and_then(|bbox_str| {
            let parts: Vec<f64> = bbox_str
                .split(',')
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() == 4 {
                Some([parts[0], parts[1], parts[2], parts[3]])
            } else {
                None
            }
        });

        if bbox_coords.is_some() {
            where_clauses.push(format!(
                "ST_Intersects(geometry, ST_MakeEnvelope(${}, ${}, ${}, ${}, 4326))",
                param_index, param_index + 1, param_index + 2, param_index + 3
            ));
            param_index += 4;
        }

        // Parse datetime filter - validate before using
        let datetime_start: Option<chrono::DateTime<chrono::FixedOffset>>;
        let datetime_end: Option<chrono::DateTime<chrono::FixedOffset>>;
        let datetime_exact: Option<chrono::DateTime<chrono::FixedOffset>>;

        if let Some(dt) = datetime {
            if dt.contains('/') {
                let parts: Vec<&str> = dt.split('/').collect();
                if parts.len() == 2 {
                    datetime_start = if parts[0] != ".." {
                        Some(chrono::DateTime::parse_from_rfc3339(parts[0])
                            .map_err(|_| AppError::BadRequest(format!("Invalid datetime start: {}", parts[0])))?)
                    } else {
                        None
                    };
                    datetime_end = if parts[1] != ".." {
                        Some(chrono::DateTime::parse_from_rfc3339(parts[1])
                            .map_err(|_| AppError::BadRequest(format!("Invalid datetime end: {}", parts[1])))?)
                    } else {
                        None
                    };
                    datetime_exact = None;

                    if datetime_start.is_some() {
                        where_clauses.push(format!("datetime >= ${}", param_index));
                        param_index += 1;
                    }
                    if datetime_end.is_some() {
                        where_clauses.push(format!("datetime <= ${}", param_index));
                        param_index += 1;
                    }
                } else {
                    datetime_start = None;
                    datetime_end = None;
                    datetime_exact = None;
                }
            } else {
                datetime_exact = Some(chrono::DateTime::parse_from_rfc3339(dt)
                    .map_err(|_| AppError::BadRequest(format!("Invalid datetime: {}", dt)))?);
                datetime_start = None;
                datetime_end = None;
                where_clauses.push(format!("datetime = ${}", param_index));
                param_index += 1;
            }
        } else {
            datetime_start = None;
            datetime_end = None;
            datetime_exact = None;
        }

        let where_clause = where_clauses.join(" AND ");

        // Count query with parameterized bindings
        let count_sql = format!(
            "SELECT COUNT(*) FROM spatialvault.items WHERE {}",
            where_clause
        );

        // Build and execute count query
        let mut count_query = sqlx::query_as::<_, (i64,)>(&count_sql)
            .bind(collection.id);

        if let Some(coords) = &bbox_coords {
            count_query = count_query
                .bind(coords[0])
                .bind(coords[1])
                .bind(coords[2])
                .bind(coords[3]);
        }

        if let Some(dt) = &datetime_start {
            count_query = count_query.bind(dt.with_timezone(&chrono::Utc));
        }
        if let Some(dt) = &datetime_end {
            count_query = count_query.bind(dt.with_timezone(&chrono::Utc));
        }
        if let Some(dt) = &datetime_exact {
            count_query = count_query.bind(dt.with_timezone(&chrono::Utc));
        }

        let count: (i64,) = count_query.fetch_one(self.db.pool()).await?;

        // Data query
        let sql = format!(
            r#"
            SELECT
                id,
                ST_AsGeoJSON(geometry)::jsonb as geometry,
                ST_XMin(geometry) as minx,
                ST_YMin(geometry) as miny,
                ST_XMax(geometry) as maxx,
                ST_YMax(geometry) as maxy,
                datetime,
                properties
            FROM spatialvault.items
            WHERE {}
            ORDER BY datetime DESC NULLS LAST, created_at DESC
            LIMIT ${} OFFSET ${}
            "#,
            where_clause, param_index, param_index + 1
        );

        // Build and execute data query
        let mut data_query = sqlx::query_as::<_, (
            Uuid,
            serde_json::Value,
            f64,
            f64,
            f64,
            f64,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
        )>(&sql)
            .bind(collection.id);

        if let Some(coords) = &bbox_coords {
            data_query = data_query
                .bind(coords[0])
                .bind(coords[1])
                .bind(coords[2])
                .bind(coords[3]);
        }

        if let Some(dt) = &datetime_start {
            data_query = data_query.bind(dt.with_timezone(&chrono::Utc));
        }
        if let Some(dt) = &datetime_end {
            data_query = data_query.bind(dt.with_timezone(&chrono::Utc));
        }
        if let Some(dt) = &datetime_exact {
            data_query = data_query.bind(dt.with_timezone(&chrono::Utc));
        }

        data_query = data_query.bind(limit as i64).bind(offset as i64);

        let rows = data_query.fetch_all(self.db.pool()).await?;

        // Get assets for all items
        let item_ids: Vec<Uuid> = rows.iter().map(|(id, ..)| *id).collect();
        let assets_map = self.get_assets_for_items(&item_ids).await?;

        let features: Vec<Feature> = rows
            .into_iter()
            .map(|(id, geometry, minx, miny, maxx, maxy, datetime, properties)| {
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

                Feature {
                    feature_type: "Feature".to_string(),
                    id: id.to_string(),
                    geometry,
                    properties: props,
                    links: None,
                    bbox: Some(vec![minx, miny, maxx, maxy]),
                    assets: Some(item_assets),
                    collection: Some(collection_id.to_string()),
                    stac_version: Some("1.0.0".to_string()),
                    stac_extensions: Some(vec![]),
                }
            })
            .collect();

        Ok((features, count.0 as usize, 4326))
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
        let collection = self.get_collection(collection_id).await?;

        match collection.collection_type.as_str() {
            "vector" => {
                self.get_vector_feature(&collection, feature_id, target_crs)
                    .await
            }
            "raster" | "pointcloud" => {
                self.get_item(&collection, collection_id, feature_id).await
            }
            _ => Err(AppError::BadRequest(format!(
                "Unknown collection type: {}",
                collection.collection_type
            ))),
        }
    }

    /// Get a vector feature
    async fn get_vector_feature(
        &self,
        collection: &Collection,
        feature_id: Uuid,
        target_crs: Option<i32>,
    ) -> AppResult<Option<(Feature, i64, i32)>> {
        let storage_srid = self.get_storage_srid(collection).await?;
        let geometry_expr = transform_geometry_sql("geometry", storage_srid, target_crs);

        let sql = format!(
            r#"
            SELECT
                id::text,
                ST_AsGeoJSON({geometry_expr})::jsonb as geometry,
                properties,
                version
            FROM {}.{}
            WHERE id = $1
            "#,
            quote_ident(&collection.schema_name),
            quote_ident(&collection.table_name),
            geometry_expr = geometry_expr
        );

        let row: Option<(String, serde_json::Value, Option<serde_json::Value>, i64)> =
            sqlx::query_as(&sql)
                .bind(feature_id)
                .fetch_optional(self.db.pool())
                .await?;

        Ok(row.map(|(id, geometry, properties, version)| {
            (
                Feature {
                    feature_type: "Feature".to_string(),
                    id,
                    geometry,
                    properties: properties.unwrap_or(serde_json::json!({})),
                    links: None,
                    bbox: None,
                    assets: None,
                    collection: None,
                    stac_version: None,
                    stac_extensions: None,
                },
                version,
                target_crs.unwrap_or(storage_srid),
            )
        }))
    }

    /// Get a raster/pointcloud item with assets
    async fn get_item(
        &self,
        collection: &Collection,
        collection_id: &str,
        item_id: Uuid,
    ) -> AppResult<Option<(Feature, i64, i32)>> {
        let sql = r#"
            SELECT
                id,
                ST_AsGeoJSON(geometry)::jsonb as geometry,
                ST_XMin(geometry) as minx,
                ST_YMin(geometry) as miny,
                ST_XMax(geometry) as maxx,
                ST_YMax(geometry) as maxy,
                datetime,
                properties,
                version
            FROM spatialvault.items
            WHERE collection_id = $1 AND id = $2
        "#;

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
        )> = sqlx::query_as(sql)
            .bind(collection.id)
            .bind(item_id)
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
                collection: Some(collection_id.to_string()),
                stac_version: Some("1.0.0".to_string()),
                stac_extensions: Some(vec![]),
            },
            version,
            4326,
        )))
    }

    pub async fn create_feature(
        &self,
        username: &str,
        collection_id: &str,
        geometry: &serde_json::Value,
        properties: &serde_json::Value,
    ) -> AppResult<(Feature, i64)> {
        let collection = self.get_collection(collection_id).await?;

        if collection.collection_type != "vector" {
            return Err(AppError::BadRequest(
                "Feature creation only available for vector collections. Use processes API for raster/pointcloud.".to_string(),
            ));
        }

        let storage_srid = self.get_storage_srid(&collection).await?;

        let sql = format!(
            r#"
            INSERT INTO {}.{} (geometry, properties)
            VALUES (ST_SetSRID(ST_GeomFromGeoJSON($1), {}), $2)
            RETURNING id::text, ST_AsGeoJSON(geometry)::jsonb, properties, version
            "#,
            quote_ident(&collection.schema_name),
            quote_ident(&collection.table_name),
            storage_srid
        );

        let (id, geom, props, version): (String, serde_json::Value, Option<serde_json::Value>, i64) =
            sqlx::query_as(&sql)
                .bind(geometry.to_string())
                .bind(properties)
                .fetch_one(self.db.pool())
                .await?;

        // Increment collection version
        sqlx::query("UPDATE spatialvault.collections SET version = version + 1 WHERE canonical_name = $1")
            .bind(collection_id)
            .execute(self.db.pool())
            .await?;

        Ok((
            Feature {
                feature_type: "Feature".to_string(),
                id,
                geometry: geom,
                properties: props.unwrap_or(serde_json::json!({})),
                links: None,
                bbox: None,
                assets: None,
                collection: None,
                stac_version: None,
                stac_extensions: None,
            },
            version,
        ))
    }

    pub async fn update_feature(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        expected_version: Option<i64>,
        geometry: Option<&serde_json::Value>,
        properties: Option<&serde_json::Value>,
    ) -> AppResult<(Feature, i64)> {
        let collection = self.get_collection(collection_id).await?;

        match collection.collection_type.as_str() {
            "vector" => {
                self.update_vector_feature(&collection, feature_id, expected_version, geometry, properties)
                    .await
            }
            "raster" | "pointcloud" => {
                self.update_item_internal(&collection, collection_id, feature_id, expected_version, geometry, properties)
                    .await
            }
            _ => Err(AppError::BadRequest(format!(
                "Unknown collection type: {}",
                collection.collection_type
            ))),
        }
    }

    async fn update_vector_feature(
        &self,
        collection: &Collection,
        feature_id: Uuid,
        expected_version: Option<i64>,
        geometry: Option<&serde_json::Value>,
        properties: Option<&serde_json::Value>,
    ) -> AppResult<(Feature, i64)> {
        let storage_srid = self.get_storage_srid(collection).await?;
        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        let mut tx = self.db.pool().begin().await?;

        // Lock and check version
        let check_sql = format!(
            r#"SELECT version FROM {}.{} WHERE id = $1 FOR UPDATE"#,
            quoted_schema, quoted_table
        );
        let current: Option<(i64,)> = sqlx::query_as(&check_sql)
            .bind(feature_id)
            .fetch_optional(&mut *tx)
            .await?;

        let current_version = current
            .ok_or_else(|| AppError::NotFound("Feature not found".to_string()))?
            .0;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current_version != version {
                return Err(AppError::PreconditionFailed(
                    "Feature has been modified".to_string(),
                ));
            }
        }

        // Build update
        let mut updates = vec!["version = version + 1", "updated_at = NOW()"];
        let mut binds: Vec<String> = Vec::new();

        if let Some(geom) = geometry {
            binds.push(geom.to_string());
            updates.push("geometry = ST_SetSRID(ST_GeomFromGeoJSON($2), storage_srid)");
        }

        // Simplified: in production, use proper parameter binding
        let update_sql = format!(
            r#"
            UPDATE {}.{}
            SET {}
            WHERE id = $1
            RETURNING id::text, ST_AsGeoJSON(geometry)::jsonb, properties, version
            "#,
            quoted_schema,
            quoted_table,
            updates.join(", ").replace("storage_srid", &storage_srid.to_string())
        );

        let (id, geom, props, version): (String, serde_json::Value, Option<serde_json::Value>, i64) =
            sqlx::query_as(&update_sql)
                .bind(feature_id)
                .fetch_one(&mut *tx)
                .await?;

        tx.commit().await?;

        Ok((
            Feature {
                feature_type: "Feature".to_string(),
                id,
                geometry: geom,
                properties: props.unwrap_or(serde_json::json!({})),
                links: None,
                bbox: None,
                assets: None,
                collection: None,
                stac_version: None,
                stac_extensions: None,
            },
            version,
        ))
    }

    async fn update_item_internal(
        &self,
        collection: &Collection,
        collection_id: &str,
        item_id: Uuid,
        expected_version: Option<i64>,
        geometry: Option<&serde_json::Value>,
        properties: Option<&serde_json::Value>,
    ) -> AppResult<(Feature, i64)> {
        let mut tx = self.db.pool().begin().await?;

        // Lock and check version
        let check_sql = r#"
            SELECT version FROM spatialvault.items
            WHERE id = $1 AND collection_id = $2
            FOR UPDATE
        "#;
        let current: Option<(i64,)> = sqlx::query_as(check_sql)
            .bind(item_id)
            .bind(&collection.id)
            .fetch_optional(&mut *tx)
            .await?;

        let current_version = current
            .ok_or_else(|| AppError::NotFound("Item not found".to_string()))?
            .0;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current_version != version {
                return Err(AppError::PreconditionFailed(
                    "Item has been modified".to_string(),
                ));
            }
        }

        // Build dynamic update
        let mut set_parts = vec!["version = version + 1", "updated_at = NOW()"];

        if geometry.is_some() {
            set_parts.push("geometry = ST_SetSRID(ST_GeomFromGeoJSON($3), 4326)");
        }

        if properties.is_some() {
            // Merge properties using JSON concatenation
            set_parts.push("properties = COALESCE(properties, '{}'::jsonb) || $4");
        }

        let update_sql = format!(
            r#"
            UPDATE spatialvault.items
            SET {}
            WHERE id = $1 AND collection_id = $2
            RETURNING id::text, ST_AsGeoJSON(geometry)::jsonb, properties, version
            "#,
            set_parts.join(", ")
        );

        let (id, geom, props, version): (String, serde_json::Value, Option<serde_json::Value>, i64) =
            sqlx::query_as(&update_sql)
                .bind(item_id)
                .bind(&collection.id)
                .bind(geometry.map(|g| g.to_string()).unwrap_or_default())
                .bind(properties)
                .fetch_one(&mut *tx)
                .await?;

        tx.commit().await?;

        // Fetch assets for the response
        let assets = self.get_item_assets(&item_id).await?;

        Ok((
            Feature {
                feature_type: "Feature".to_string(),
                id,
                geometry: geom,
                properties: props.unwrap_or(serde_json::json!({})),
                links: None,
                bbox: None,
                assets: Some(assets),
                collection: Some(collection_id.to_string()),
                stac_version: Some("1.0.0".to_string()),
                stac_extensions: Some(vec![]),
            },
            version,
        ))
    }

    pub async fn replace_feature(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        expected_version: Option<i64>,
        geometry: &serde_json::Value,
        properties: &serde_json::Value,
    ) -> AppResult<(Feature, i64)> {
        let collection = self.get_collection(collection_id).await?;

        match collection.collection_type.as_str() {
            "vector" => {
                self.replace_vector_feature(&collection, feature_id, expected_version, geometry, properties)
                    .await
            }
            "raster" | "pointcloud" => {
                self.replace_item_internal(&collection, collection_id, feature_id, expected_version, geometry, properties)
                    .await
            }
            _ => Err(AppError::BadRequest(format!(
                "Unknown collection type: {}",
                collection.collection_type
            ))),
        }
    }

    async fn replace_vector_feature(
        &self,
        collection: &Collection,
        feature_id: Uuid,
        expected_version: Option<i64>,
        geometry: &serde_json::Value,
        properties: &serde_json::Value,
    ) -> AppResult<(Feature, i64)> {
        let storage_srid = self.get_storage_srid(collection).await?;

        let mut tx = self.db.pool().begin().await?;

        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        // Check version
        let check_sql = format!(
            r#"SELECT version FROM {}.{} WHERE id = $1 FOR UPDATE"#,
            quoted_schema, quoted_table
        );
        let current: Option<(i64,)> = sqlx::query_as(&check_sql)
            .bind(feature_id)
            .fetch_optional(&mut *tx)
            .await?;

        let current_version = current
            .ok_or_else(|| AppError::NotFound("Feature not found".to_string()))?
            .0;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current_version != version {
                return Err(AppError::PreconditionFailed(
                    "Feature has been modified".to_string(),
                ));
            }
        }

        let sql = format!(
            r#"
            UPDATE {}.{}
            SET
                geometry = ST_SetSRID(ST_GeomFromGeoJSON($2), {}),
                properties = $3,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id::text, ST_AsGeoJSON(geometry)::jsonb, properties, version
            "#,
            quoted_schema, quoted_table, storage_srid
        );

        let (id, geom, props, version): (String, serde_json::Value, Option<serde_json::Value>, i64) =
            sqlx::query_as(&sql)
                .bind(feature_id)
                .bind(geometry.to_string())
                .bind(properties)
                .fetch_one(&mut *tx)
                .await?;

        tx.commit().await?;

        Ok((
            Feature {
                feature_type: "Feature".to_string(),
                id,
                geometry: geom,
                properties: props.unwrap_or(serde_json::json!({})),
                links: None,
                bbox: None,
                assets: None,
                collection: None,
                stac_version: None,
                stac_extensions: None,
            },
            version,
        ))
    }

    async fn replace_item_internal(
        &self,
        collection: &Collection,
        collection_id: &str,
        item_id: Uuid,
        expected_version: Option<i64>,
        geometry: &serde_json::Value,
        properties: &serde_json::Value,
    ) -> AppResult<(Feature, i64)> {
        let mut tx = self.db.pool().begin().await?;

        // Check version
        let check_sql = r#"
            SELECT version FROM spatialvault.items
            WHERE id = $1 AND collection_id = $2
            FOR UPDATE
        "#;
        let current: Option<(i64,)> = sqlx::query_as(check_sql)
            .bind(item_id)
            .bind(&collection.id)
            .fetch_optional(&mut *tx)
            .await?;

        let current_version = current
            .ok_or_else(|| AppError::NotFound("Item not found".to_string()))?
            .0;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current_version != version {
                return Err(AppError::PreconditionFailed(
                    "Item has been modified".to_string(),
                ));
            }
        }

        // Extract datetime from properties if present
        let datetime = properties
            .get("datetime")
            .and_then(|d| d.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let sql = r#"
            UPDATE spatialvault.items
            SET
                geometry = ST_SetSRID(ST_GeomFromGeoJSON($3), 4326),
                properties = $4,
                datetime = $5,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1 AND collection_id = $2
            RETURNING id::text, ST_AsGeoJSON(geometry)::jsonb, properties, version
        "#;

        let (id, geom, props, version): (String, serde_json::Value, Option<serde_json::Value>, i64) =
            sqlx::query_as(sql)
                .bind(item_id)
                .bind(&collection.id)
                .bind(geometry.to_string())
                .bind(properties)
                .bind(datetime)
                .fetch_one(&mut *tx)
                .await?;

        tx.commit().await?;

        // Fetch assets for the response
        let assets = self.get_item_assets(&item_id).await?;

        Ok((
            Feature {
                feature_type: "Feature".to_string(),
                id,
                geometry: geom,
                properties: props.unwrap_or(serde_json::json!({})),
                links: None,
                bbox: None,
                assets: Some(assets),
                collection: Some(collection_id.to_string()),
                stac_version: Some("1.0.0".to_string()),
                stac_extensions: Some(vec![]),
            },
            version,
        ))
    }

    pub async fn delete_feature(
        &self,
        username: &str,
        collection_id: &str,
        feature_id: Uuid,
        expected_version: Option<i64>,
    ) -> AppResult<()> {
        let collection = self.get_collection(collection_id).await?;

        match collection.collection_type.as_str() {
            "vector" => {
                self.delete_vector_feature(&collection, feature_id, expected_version)
                    .await
            }
            "raster" | "pointcloud" => {
                self.delete_item_internal(&collection, feature_id, expected_version)
                    .await
            }
            _ => Err(AppError::BadRequest(format!(
                "Unknown collection type: {}",
                collection.collection_type
            ))),
        }
    }

    async fn delete_vector_feature(
        &self,
        collection: &Collection,
        feature_id: Uuid,
        expected_version: Option<i64>,
    ) -> AppResult<()> {
        let mut tx = self.db.pool().begin().await?;

        let quoted_schema = quote_ident(&collection.schema_name);
        let quoted_table = quote_ident(&collection.table_name);

        // Check version
        let check_sql = format!(
            r#"SELECT version FROM {}.{} WHERE id = $1 FOR UPDATE"#,
            quoted_schema, quoted_table
        );
        let current: Option<(i64,)> = sqlx::query_as(&check_sql)
            .bind(feature_id)
            .fetch_optional(&mut *tx)
            .await?;

        let current_version = current
            .ok_or_else(|| AppError::NotFound("Feature not found".to_string()))?
            .0;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current_version != version {
                return Err(AppError::PreconditionFailed(
                    "Feature has been modified".to_string(),
                ));
            }
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

    async fn delete_item_internal(
        &self,
        collection: &Collection,
        item_id: Uuid,
        expected_version: Option<i64>,
    ) -> AppResult<()> {
        let mut tx = self.db.pool().begin().await?;

        // Check version and that item belongs to this collection
        let check_sql = r#"
            SELECT version FROM spatialvault.items
            WHERE id = $1 AND collection_id = $2
            FOR UPDATE
        "#;
        let current: Option<(i64,)> = sqlx::query_as(check_sql)
            .bind(item_id)
            .bind(&collection.id)
            .fetch_optional(&mut *tx)
            .await?;

        let current_version = current
            .ok_or_else(|| AppError::NotFound("Item not found".to_string()))?
            .0;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current_version != version {
                return Err(AppError::PreconditionFailed(
                    "Item has been modified".to_string(),
                ));
            }
        }

        // Delete assets first (cascading would handle this, but be explicit)
        sqlx::query("DELETE FROM spatialvault.assets WHERE item_id = $1")
            .bind(item_id)
            .execute(&mut *tx)
            .await?;

        // Delete item
        sqlx::query("DELETE FROM spatialvault.items WHERE id = $1")
            .bind(item_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Create a STAC item (for raster/pointcloud collections)
    pub async fn create_item(
        &self,
        _username: &str,
        collection_id: &str,
        geometry: &serde_json::Value,
        properties: &serde_json::Value,
        datetime: Option<chrono::DateTime<chrono::Utc>>,
        assets: Option<&serde_json::Value>,
    ) -> AppResult<(Feature, i64)> {
        let collection = self.get_collection(collection_id).await?;

        if collection.collection_type == "vector" {
            return Err(AppError::BadRequest(
                "Item creation with assets requires raster/pointcloud collection. Use feature creation for vector collections.".to_string(),
            ));
        }

        let item_id = Uuid::new_v4();

        let mut tx = self.db.pool().begin().await?;

        // Insert item
        let sql = r#"
            INSERT INTO spatialvault.items (id, collection_id, geometry, datetime, properties)
            VALUES ($1, $2, ST_SetSRID(ST_GeomFromGeoJSON($3), 4326), $4, $5)
            RETURNING id, ST_AsGeoJSON(geometry)::jsonb, ST_XMin(geometry), ST_YMin(geometry),
                      ST_XMax(geometry), ST_YMax(geometry), datetime, properties, version
        "#;

        let (id, geom, minx, miny, maxx, maxy, dt, props, version): (
            Uuid,
            serde_json::Value,
            f64,
            f64,
            f64,
            f64,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
            i64,
        ) = sqlx::query_as(sql)
            .bind(item_id)
            .bind(collection.id)
            .bind(geometry.to_string())
            .bind(datetime)
            .bind(properties)
            .fetch_one(&mut *tx)
            .await?;

        // Insert assets if provided
        if let Some(assets_obj) = assets {
            if let serde_json::Value::Object(assets_map) = assets_obj {
                for (key, asset) in assets_map {
                    let href = asset.get("href").and_then(|v| v.as_str());
                    if let Some(href) = href {
                        let media_type = asset.get("type").and_then(|v| v.as_str());
                        let title = asset.get("title").and_then(|v| v.as_str());
                        let description = asset.get("description").and_then(|v| v.as_str());
                        let roles: Option<Vec<String>> = asset
                            .get("roles")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());
                        let file_size = asset.get("file:size").and_then(|v| v.as_i64());

                        sqlx::query(
                            r#"
                            INSERT INTO spatialvault.assets (item_id, key, href, type, title, description, roles, file_size)
                            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                            "#,
                        )
                        .bind(item_id)
                        .bind(key)
                        .bind(href)
                        .bind(media_type)
                        .bind(title)
                        .bind(description)
                        .bind(roles)
                        .bind(file_size)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
            }
        }

        // Increment collection version
        sqlx::query("UPDATE spatialvault.collections SET version = version + 1 WHERE id = $1")
            .bind(collection.id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        // Get assets for response
        let assets_map = self.get_assets_for_items(&[id]).await?;
        let item_assets = assets_map.get(&id).cloned().unwrap_or_else(|| serde_json::json!({}));

        let mut final_props = props.unwrap_or(serde_json::json!({}));
        if let Some(dt) = dt {
            if let serde_json::Value::Object(ref mut map) = final_props {
                map.insert("datetime".to_string(), serde_json::json!(dt.to_rfc3339()));
            }
        }

        Ok((
            Feature {
                feature_type: "Feature".to_string(),
                id: id.to_string(),
                geometry: geom,
                properties: final_props,
                links: None,
                bbox: Some(vec![minx, miny, maxx, maxy]),
                assets: Some(item_assets),
                collection: Some(collection_id.to_string()),
                stac_version: Some("1.0.0".to_string()),
                stac_extensions: Some(vec![]),
            },
            version,
        ))
    }

    /// Update a STAC item (PATCH - JSON Merge Patch)
    pub async fn update_item(
        &self,
        _username: &str,
        collection_id: &str,
        item_id: Uuid,
        expected_version: Option<i64>,
        geometry: Option<&serde_json::Value>,
        properties: Option<&serde_json::Value>,
        datetime: Option<chrono::DateTime<chrono::Utc>>,
        assets: Option<&serde_json::Value>,
    ) -> AppResult<(Feature, i64)> {
        let collection = self.get_collection(collection_id).await?;

        let mut tx = self.db.pool().begin().await?;

        // Lock and check version
        let current: Option<(i64,)> = sqlx::query_as(
            "SELECT version FROM spatialvault.items WHERE collection_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(collection.id)
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?;

        let current_version = current
            .ok_or_else(|| AppError::NotFound("Item not found".to_string()))?
            .0;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current_version != version {
                return Err(AppError::PreconditionFailed(
                    "Item has been modified".to_string(),
                ));
            }
        }

        // Build update dynamically
        let mut set_clauses = vec!["version = version + 1", "updated_at = NOW()"];

        if geometry.is_some() {
            set_clauses.push("geometry = ST_SetSRID(ST_GeomFromGeoJSON($3), 4326)");
        }
        if properties.is_some() {
            set_clauses.push("properties = COALESCE(properties, '{}'::jsonb) || $4");
        }
        if datetime.is_some() {
            set_clauses.push("datetime = $5");
        }

        let update_sql = format!(
            r#"
            UPDATE spatialvault.items
            SET {}
            WHERE collection_id = $1 AND id = $2
            RETURNING id, ST_AsGeoJSON(geometry)::jsonb, ST_XMin(geometry), ST_YMin(geometry),
                      ST_XMax(geometry), ST_YMax(geometry), datetime, properties, version
            "#,
            set_clauses.join(", ")
        );

        let mut query = sqlx::query_as::<_, (Uuid, serde_json::Value, f64, f64, f64, f64, Option<chrono::DateTime<chrono::Utc>>, Option<serde_json::Value>, i64)>(&update_sql)
            .bind(collection.id)
            .bind(item_id);

        if let Some(geom) = geometry {
            query = query.bind(geom.to_string());
        } else {
            query = query.bind(Option::<String>::None);
        }

        if let Some(props) = properties {
            query = query.bind(props);
        } else {
            query = query.bind(Option::<serde_json::Value>::None);
        }

        query = query.bind(datetime);

        let (id, geom, minx, miny, maxx, maxy, dt, props, version) = query
            .fetch_one(&mut *tx)
            .await?;

        // Update assets if provided (merge semantics)
        if let Some(assets_obj) = assets {
            if let serde_json::Value::Object(assets_map) = assets_obj {
                for (key, asset) in assets_map {
                    if asset.is_null() {
                        // Delete asset
                        sqlx::query("DELETE FROM spatialvault.assets WHERE item_id = $1 AND key = $2")
                            .bind(item_id)
                            .bind(key)
                            .execute(&mut *tx)
                            .await?;
                    } else if let Some(href) = asset.get("href").and_then(|v| v.as_str()) {
                        // Upsert asset
                        let media_type = asset.get("type").and_then(|v| v.as_str());
                        let title = asset.get("title").and_then(|v| v.as_str());
                        let description = asset.get("description").and_then(|v| v.as_str());
                        let roles: Option<Vec<String>> = asset
                            .get("roles")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());
                        let file_size = asset.get("file:size").and_then(|v| v.as_i64());

                        sqlx::query(
                            r#"
                            INSERT INTO spatialvault.assets (item_id, key, href, type, title, description, roles, file_size)
                            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                            ON CONFLICT (item_id, key) DO UPDATE SET
                                href = EXCLUDED.href,
                                type = EXCLUDED.type,
                                title = EXCLUDED.title,
                                description = EXCLUDED.description,
                                roles = EXCLUDED.roles,
                                file_size = EXCLUDED.file_size
                            "#,
                        )
                        .bind(item_id)
                        .bind(key)
                        .bind(href)
                        .bind(media_type)
                        .bind(title)
                        .bind(description)
                        .bind(roles)
                        .bind(file_size)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
            }
        }

        tx.commit().await?;

        // Get assets for response
        let assets_map = self.get_assets_for_items(&[id]).await?;
        let item_assets = assets_map.get(&id).cloned().unwrap_or_else(|| serde_json::json!({}));

        let mut final_props = props.unwrap_or(serde_json::json!({}));
        if let Some(dt) = dt {
            if let serde_json::Value::Object(ref mut map) = final_props {
                map.insert("datetime".to_string(), serde_json::json!(dt.to_rfc3339()));
            }
        }

        Ok((
            Feature {
                feature_type: "Feature".to_string(),
                id: id.to_string(),
                geometry: geom,
                properties: final_props,
                links: None,
                bbox: Some(vec![minx, miny, maxx, maxy]),
                assets: Some(item_assets),
                collection: Some(collection_id.to_string()),
                stac_version: Some("1.0.0".to_string()),
                stac_extensions: Some(vec![]),
            },
            version,
        ))
    }

    /// Replace a STAC item (PUT)
    pub async fn replace_item(
        &self,
        _username: &str,
        collection_id: &str,
        item_id: Uuid,
        expected_version: Option<i64>,
        geometry: &serde_json::Value,
        properties: &serde_json::Value,
        datetime: Option<chrono::DateTime<chrono::Utc>>,
        assets: Option<&serde_json::Value>,
    ) -> AppResult<(Feature, i64)> {
        let collection = self.get_collection(collection_id).await?;

        let mut tx = self.db.pool().begin().await?;

        // Lock and check version
        let current: Option<(i64,)> = sqlx::query_as(
            "SELECT version FROM spatialvault.items WHERE collection_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(collection.id)
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?;

        let current_version = current
            .ok_or_else(|| AppError::NotFound("Item not found".to_string()))?
            .0;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current_version != version {
                return Err(AppError::PreconditionFailed(
                    "Item has been modified".to_string(),
                ));
            }
        }

        // Replace item
        let sql = r#"
            UPDATE spatialvault.items
            SET geometry = ST_SetSRID(ST_GeomFromGeoJSON($3), 4326),
                datetime = $4,
                properties = $5,
                version = version + 1,
                updated_at = NOW()
            WHERE collection_id = $1 AND id = $2
            RETURNING id, ST_AsGeoJSON(geometry)::jsonb, ST_XMin(geometry), ST_YMin(geometry),
                      ST_XMax(geometry), ST_YMax(geometry), datetime, properties, version
        "#;

        let (id, geom, minx, miny, maxx, maxy, dt, props, version): (
            Uuid,
            serde_json::Value,
            f64,
            f64,
            f64,
            f64,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
            i64,
        ) = sqlx::query_as(sql)
            .bind(collection.id)
            .bind(item_id)
            .bind(geometry.to_string())
            .bind(datetime)
            .bind(properties)
            .fetch_one(&mut *tx)
            .await?;

        // Delete all existing assets and insert new ones
        sqlx::query("DELETE FROM spatialvault.assets WHERE item_id = $1")
            .bind(item_id)
            .execute(&mut *tx)
            .await?;

        if let Some(assets_obj) = assets {
            if let serde_json::Value::Object(assets_map) = assets_obj {
                for (key, asset) in assets_map {
                    if let Some(href) = asset.get("href").and_then(|v| v.as_str()) {
                        let media_type = asset.get("type").and_then(|v| v.as_str());
                        let title = asset.get("title").and_then(|v| v.as_str());
                        let description = asset.get("description").and_then(|v| v.as_str());
                        let roles: Option<Vec<String>> = asset
                            .get("roles")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());
                        let file_size = asset.get("file:size").and_then(|v| v.as_i64());

                        sqlx::query(
                            r#"
                            INSERT INTO spatialvault.assets (item_id, key, href, type, title, description, roles, file_size)
                            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                            "#,
                        )
                        .bind(item_id)
                        .bind(key)
                        .bind(href)
                        .bind(media_type)
                        .bind(title)
                        .bind(description)
                        .bind(roles)
                        .bind(file_size)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
            }
        }

        tx.commit().await?;

        // Get assets for response
        let assets_map = self.get_assets_for_items(&[id]).await?;
        let item_assets = assets_map.get(&id).cloned().unwrap_or_else(|| serde_json::json!({}));

        let mut final_props = props.unwrap_or(serde_json::json!({}));
        if let Some(dt) = dt {
            if let serde_json::Value::Object(ref mut map) = final_props {
                map.insert("datetime".to_string(), serde_json::json!(dt.to_rfc3339()));
            }
        }

        Ok((
            Feature {
                feature_type: "Feature".to_string(),
                id: id.to_string(),
                geometry: geom,
                properties: final_props,
                links: None,
                bbox: Some(vec![minx, miny, maxx, maxy]),
                assets: Some(item_assets),
                collection: Some(collection_id.to_string()),
                stac_version: Some("1.0.0".to_string()),
                stac_extensions: Some(vec![]),
            },
            version,
        ))
    }

    /// Delete a STAC item
    pub async fn delete_item(
        &self,
        _username: &str,
        collection_id: &str,
        item_id: Uuid,
        expected_version: Option<i64>,
    ) -> AppResult<()> {
        let collection = self.get_collection(collection_id).await?;

        let mut tx = self.db.pool().begin().await?;

        // Lock and check version
        let current: Option<(i64,)> = sqlx::query_as(
            "SELECT version FROM spatialvault.items WHERE collection_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(collection.id)
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?;

        let current_version = current
            .ok_or_else(|| AppError::NotFound("Item not found".to_string()))?
            .0;

        // Check version if If-Match header was provided
        if let Some(version) = expected_version {
            if current_version != version {
                return Err(AppError::PreconditionFailed(
                    "Item has been modified".to_string(),
                ));
            }
        }

        // Delete item (assets cascade)
        sqlx::query("DELETE FROM spatialvault.items WHERE id = $1")
            .bind(item_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    async fn get_collection(&self, collection_id: &str) -> AppResult<Collection> {
        sqlx::query_as("SELECT * FROM spatialvault.collections WHERE canonical_name = $1")
            .bind(collection_id)
            .fetch_optional(self.db.pool())
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))
    }

    async fn get_storage_srid(&self, collection: &Collection) -> AppResult<i32> {
        // Get SRID from geometry column definition
        let sql = format!(
            r#"
            SELECT srid FROM geometry_columns
            WHERE f_table_schema = $1 AND f_table_name = $2
            "#
        );

        let result: Option<(i32,)> = sqlx::query_as(&sql)
            .bind(&collection.schema_name)
            .bind(&collection.table_name)
            .fetch_optional(self.db.pool())
            .await?;

        Ok(result.map(|(srid,)| srid).unwrap_or(4326))
    }
}

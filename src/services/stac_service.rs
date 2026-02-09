use futures::future::try_join_all;
use itertools::Itertools;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::common::{Assets, GeoJsonGeometry, Link, media_type, rel};
use crate::api::stac::item::{StacItem, StacItemProperties, StacSearchParams};
use crate::auth::quote_ident;
use crate::db::Database;
use crate::error::AppResult;
use crate::services::{CollectionService, FeatureService};

pub struct StacSearchResult {
    pub items: Vec<StacItem>,
    pub returned: u32,
    pub matched: Option<u64>,
}

pub struct StacService {
    db: Arc<Database>,
    base_url: String,
    collection_service: Arc<CollectionService>,
    feature_service: Arc<FeatureService>,
}

impl StacService {
    pub fn new(
        db: Arc<Database>,
        base_url: String,
        collection_service: Arc<CollectionService>,
        feature_service: Arc<FeatureService>,
    ) -> Self {
        Self {
            db,
            base_url,
            collection_service,
            feature_service,
        }
    }

    pub async fn search(
        &self,
        username: &str,
        params: &StacSearchParams,
    ) -> AppResult<StacSearchResult> {
        let mut where_clauses = vec!["TRUE".to_string()];

        // Filter by collections
        let tables: Vec<(String, String, String)> = if let Some(ref collections) =
            params.collections
        {
            let collection_list: Vec<&str> = collections.split(',').map(|s| s.trim()).collect();
            sqlx::query_as("SELECT canonical_name, schema_name, table_name FROM spatialvault.collections WHERE canonical_name = ANY($1)")
                .bind(&collection_list)
                .fetch_all(self.db.pool())
                .await?
        } else {
            sqlx::query_as(
                "SELECT canonical_name, schema_name, table_name FROM spatialvault.collections",
            )
            .fetch_all(self.db.pool())
            .await?
        };
        let tables: Vec<_> = tables
            .into_iter()
            .map(|(name, schema, table)| {
                (
                    name,
                    format!("{}.{}", quote_ident(&schema), quote_ident(&table)),
                )
            })
            .collect();

        // Filter by item IDs
        if let Some(ref ids) = params.ids {
            let id_list: Vec<&str> = ids.split(',').map(|s| s.trim()).collect();
            let quoted: Vec<String> = id_list
                .iter()
                .map(|s| format!("'{}'", s.replace('\'', "''")))
                .collect();
            where_clauses.push(format!("i.id::text IN ({})", quoted.join(", ")));
        }

        // Filter by bbox
        if let Some(ref bbox) = params.bbox {
            let parts: Vec<f64> = bbox
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if parts.len() == 4 {
                where_clauses.push(format!(
                    "ST_Intersects(i.geometry, ST_MakeEnvelope({}, {}, {}, {}, 4326))",
                    parts[0], parts[1], parts[2], parts[3]
                ));
            }
        }

        // Filter by datetime
        if let Some(ref datetime) = params.datetime {
            if datetime.contains('/') {
                let parts: Vec<&str> = datetime.split('/').collect();
                if parts.len() == 2 {
                    if parts[0] != ".." {
                        where_clauses.push(format!("i.datetime >= '{}'", parts[0]));
                    }
                    if parts[1] != ".." {
                        where_clauses.push(format!("i.datetime <= '{}'", parts[1]));
                    }
                }
            } else {
                where_clauses.push(format!("i.datetime = '{}'", datetime));
            }
        }

        let where_clause = where_clauses.join(" AND ");

        // Count query
        let count_sql = tables
            .iter()
            .map(|(_, table)| format!("SELECT COUNT(*) FROM {} WHERE {}", table, where_clause))
            .join(" UNION ALL ");
        let count: (i64,) = sqlx::query_as(&count_sql).fetch_one(self.db.pool()).await?;

        // Data query - get items
        let sql = tables
            .iter()
            .map(|(collection, table)| {
                format!(
                    r#"
            SELECT
                id,
                '{}' as collection_name,
                ST_AsGeoJSON(geometry)::jsonb as geometry,
                ST_XMin(geometry) as minx,
                ST_YMin(geometry) as miny,
                ST_XMax(geometry) as maxx,
                ST_YMax(geometry) as maxy,
                datetime,
                properties
            FROM {}
            WHERE {}
            ORDER BY i.datetime DESC NULLS LAST
            LIMIT {} OFFSET 0
            "#,
                    collection, table, where_clause, params.limit
                )
            })
            .join(" UNION ALL ");

        let rows: Vec<(
            Uuid,
            String,
            sqlx::types::Json<GeoJsonGeometry>,
            f64,
            f64,
            f64,
            f64,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
        )> = sqlx::query_as(&sql).fetch_all(self.db.pool()).await?;

        // Fetch assets for all items
        // Collect groups into owned data first so the async futures are Send
        let grouped: Vec<(String, Vec<Uuid>)> = rows
            .iter()
            .chunk_by(|(_, collection, ..)| collection.clone())
            .into_iter()
            .map(|(collection, group)| {
                (collection, group.map(|(id, ..)| *id).collect::<Vec<Uuid>>())
            })
            .collect();

        let assets_map: HashMap<_, _> = try_join_all(
            grouped.into_iter().map(|(collection_name, ids)| async move {
                let collection = self
                    .collection_service
                    .get_collection(username, &collection_name)
                    .await?
                    .as_collection();
                AppResult::<(Uuid, HashMap<Uuid, Assets>)>::Ok((
                    collection.id,
                    self.feature_service
                        .get_assets_for_items(&collection, &ids)
                        .await?,
                ))
            }),
        )
        .await?
        .into_iter()
        .collect();

        let items: Vec<StacItem> = rows
            .into_iter()
            .map(
                |(id, collection, geometry, minx, miny, maxx, maxy, datetime, properties)| {
                    let item_assets = assets_map
                        .get(&id)
                        .and_then(|m| m.get(&id).cloned())
                        .unwrap_or_default();

                    let id_str = id.to_string();

                    StacItem {
                        item_type: "Feature".to_string(),
                        stac_version: "1.0.0".to_string(),
                        stac_extensions: vec![],
                        id: id_str.clone(),
                        geometry: geometry.0,
                        bbox: Some(vec![minx, miny, maxx, maxy]),
                        properties: StacItemProperties {
                            datetime: datetime.map(|dt| dt.to_rfc3339()),
                            additional: properties.unwrap_or(serde_json::json!({})),
                        },
                        links: vec![
                            Link::new(
                                format!(
                                    "{}/collections/{}/items/{}",
                                    self.base_url, collection, id_str
                                ),
                                rel::SELF,
                            )
                            .with_type(media_type::GEOJSON),
                            Link::new(
                                format!("{}/collections/{}", self.base_url, collection),
                                rel::COLLECTION,
                            )
                            .with_type(media_type::JSON),
                            Link::new(format!("{}/stac", self.base_url), rel::ROOT)
                                .with_type(media_type::JSON),
                        ],
                        assets: item_assets,
                        collection,
                    }
                },
            )
            .collect();

        Ok(StacSearchResult {
            returned: items.len() as u32,
            matched: Some(count.0 as u64),
            items,
        })
    }
}

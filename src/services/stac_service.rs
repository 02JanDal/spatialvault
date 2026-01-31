use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::common::{Link, media_type, rel};
use crate::api::stac::item::{StacItem, StacItemProperties, StacSearchParams};
use crate::db::Database;
use crate::error::AppResult;

pub struct StacSearchResult {
    pub items: Vec<StacItem>,
    pub returned: u32,
    pub matched: Option<u64>,
}

/// STAC Asset representation
#[derive(Debug, serde::Serialize)]
pub struct StacAsset {
    pub href: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    #[serde(rename = "file:size", skip_serializing_if = "Option::is_none")]
    pub file_size: Option<i64>,
}

pub struct StacService {
    db: Arc<Database>,
    base_url: String,
}

impl StacService {
    pub fn new(db: Arc<Database>, base_url: String) -> Self {
        Self { db, base_url }
    }

    pub async fn search(
        &self,
        username: &str,
        params: &StacSearchParams,
    ) -> AppResult<StacSearchResult> {
        let mut where_clauses = vec!["TRUE".to_string()];

        // Filter by collections
        if let Some(ref collections) = params.collections {
            let collection_list: Vec<&str> = collections.split(',').map(|s| s.trim()).collect();
            let quoted: Vec<String> = collection_list
                .iter()
                .map(|s| format!("'{}'", s.replace('\'', "''")))
                .collect();
            where_clauses.push(format!("c.canonical_name IN ({})", quoted.join(", ")));
        }

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
        let count_sql = format!(
            r#"
            SELECT COUNT(*)
            FROM spatialvault.items i
            JOIN spatialvault.collections c ON i.collection_id = c.id
            WHERE {}
            "#,
            where_clause
        );

        let count: (i64,) = sqlx::query_as(&count_sql).fetch_one(self.db.pool()).await?;

        // Data query - get items
        let sql = format!(
            r#"
            SELECT
                i.id,
                c.canonical_name as collection_name,
                ST_AsGeoJSON(i.geometry)::jsonb as geometry,
                ST_XMin(i.geometry) as minx,
                ST_YMin(i.geometry) as miny,
                ST_XMax(i.geometry) as maxx,
                ST_YMax(i.geometry) as maxy,
                i.datetime,
                i.properties
            FROM spatialvault.items i
            JOIN spatialvault.collections c ON i.collection_id = c.id
            WHERE {}
            ORDER BY i.datetime DESC NULLS LAST
            LIMIT {} OFFSET 0
            "#,
            where_clause, params.limit
        );

        let rows: Vec<(
            Uuid,
            String,
            serde_json::Value,
            f64,
            f64,
            f64,
            f64,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
        )> = sqlx::query_as(&sql).fetch_all(self.db.pool()).await?;

        // Collect item IDs for asset lookup
        let item_ids: Vec<Uuid> = rows.iter().map(|(id, ..)| *id).collect();

        // Fetch assets for all items
        let assets_map = self.get_assets_for_items(&item_ids).await?;

        let items: Vec<StacItem> = rows
            .into_iter()
            .map(
                |(id, collection, geometry, minx, miny, maxx, maxy, datetime, properties)| {
                    let item_assets = assets_map
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));

                    let id_str = id.to_string();

                    StacItem {
                        item_type: "Feature".to_string(),
                        stac_version: "1.0.0".to_string(),
                        stac_extensions: vec![],
                        id: id_str.clone(),
                        geometry,
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

    /// Get assets for a list of item IDs
    async fn get_assets_for_items(
        &self,
        item_ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, serde_json::Value>> {
        if item_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Build placeholders for IN clause
        let placeholders: Vec<String> = item_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect();

        let sql = format!(
            r#"
            SELECT item_id, key, href, type, title, description, roles, file_size, extra_fields
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
                Option<serde_json::Value>,
            ),
        >(&sql);

        for id in item_ids {
            query = query.bind(id);
        }

        let rows = query.fetch_all(self.db.pool()).await?;

        // Group assets by item_id
        let mut assets_map: HashMap<Uuid, HashMap<String, StacAsset>> = HashMap::new();

        for (item_id, key, href, media_type, title, description, roles, file_size, _extra) in rows {
            let asset = StacAsset {
                href,
                media_type,
                title,
                description,
                roles,
                file_size,
            };

            assets_map.entry(item_id).or_default().insert(key, asset);
        }

        // Convert to JSON values
        let result: HashMap<Uuid, serde_json::Value> = assets_map
            .into_iter()
            .map(|(id, assets)| {
                (
                    id,
                    serde_json::to_value(assets).unwrap_or(serde_json::json!({})),
                )
            })
            .collect();

        Ok(result)
    }
}

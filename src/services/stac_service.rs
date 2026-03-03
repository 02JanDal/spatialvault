use futures::future::try_join_all;
use itertools::Itertools;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::common::{Assets, GeoJsonGeometry, Link, media_type, rel};
use crate::api::stac::item::{StacItem, StacItemProperties, StacSearchParams};
use crate::auth::quote_ident;
use crate::db::Database;
use crate::error::{AppResult, BadRequest};
use crate::services::{CollectionService, FeatureService};

/// Validate an RFC 3339 datetime string, returning BadRequest on failure.
fn parse_rfc3339(s: &str) -> AppResult<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(s).map_err(|_| {
        BadRequest {
            message: format!("Invalid datetime: {}", s),
        }
        .build()
    })
}

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
        let tables: Vec<(String, String, String, String)> = if let Some(ref collections) =
            params.collections
        {
            let collection_list: Vec<&str> = collections.split(',').map(|s| s.trim()).collect();
            sqlx::query_as("SELECT canonical_name, schema_name, table_name, collection_type FROM spatialvault.collections WHERE canonical_name = ANY($1)")
                .bind(&collection_list)
                .fetch_all(self.db.pool())
                .await?
        } else {
            sqlx::query_as(
                "SELECT canonical_name, schema_name, table_name, collection_type FROM spatialvault.collections",
            )
            .fetch_all(self.db.pool())
            .await?
        };
        let tables: Vec<_> = tables
            .into_iter()
            .map(|(name, schema, table, ctype)| {
                let has_datetime = ctype == "raster" || ctype == "pointcloud";
                (
                    name,
                    format!("{}.{}", quote_ident(&schema), quote_ident(&table)),
                    has_datetime,
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
            where_clauses.push(format!("i._id::text IN ({})", quoted.join(", ")));
        }

        // bbox and intersects are mutually exclusive
        if params.bbox.is_some() && params.intersects.is_some() {
            return Err(BadRequest {
                message: "bbox and intersects are mutually exclusive".to_string(),
            }
            .build());
        }

        // Filter by bbox
        if let Some(ref bbox) = params.bbox {
            let trimmed = bbox.trim();
            if trimmed.contains('[') || trimmed.contains(']') {
                return Err(BadRequest {
                    message: "bbox must not contain brackets".to_string(),
                }
                .build());
            }
            let parts: Result<Vec<f64>, _> = trimmed
                .split(',')
                .map(|s| s.trim().parse::<f64>())
                .collect();
            let parts = parts.map_err(|_| {
                BadRequest {
                    message: "bbox values must be numbers".to_string(),
                }
                .build()
            })?;
            if parts.len() != 4 && parts.len() != 6 {
                return Err(BadRequest {
                    message: format!("bbox must have 4 or 6 values, got {}", parts.len()),
                }
                .build());
            }
            if parts[1] > parts[3] {
                return Err(BadRequest {
                    message: "bbox south latitude must not exceed north latitude".to_string(),
                }
                .build());
            }
            where_clauses.push(format!(
                "ST_Intersects(i.geometry, ST_MakeEnvelope({}, {}, {}, {}, 4326))",
                parts[0], parts[1], parts[2], parts[3]
            ));
        }

        // Filter by intersects geometry
        if let Some(ref intersects) = params.intersects {
            where_clauses.push(format!(
                "ST_Intersects(i.geometry, ST_GeomFromGeoJSON('{}'))",
                intersects.replace('\'', "''")
            ));
        }

        // Filter by datetime (only applied to tables with _datetime column)
        let mut datetime_clauses: Vec<String> = Vec::new();
        if let Some(ref datetime) = params.datetime {
            if datetime.contains('/') {
                let parts: Vec<&str> = datetime.split('/').collect();
                if parts.len() != 2 {
                    return Err(BadRequest {
                        message: "Invalid datetime interval".to_string(),
                    }
                    .build());
                }
                // Empty string or ".." means open-ended
                let is_open = |s: &str| s.is_empty() || s == "..";
                if is_open(parts[0]) && is_open(parts[1]) {
                    return Err(BadRequest {
                        message: "Invalid datetime interval: both bounds cannot be open".to_string(),
                    }
                    .build());
                }
                let start = if !is_open(parts[0]) {
                    let dt = parse_rfc3339(parts[0])?;
                    datetime_clauses.push(format!("i._datetime >= '{}'", parts[0]));
                    Some(dt)
                } else {
                    None
                };
                if !is_open(parts[1]) {
                    let dt = parse_rfc3339(parts[1])?;
                    if let Some(start_dt) = start {
                        if start_dt > dt {
                            return Err(BadRequest {
                                message: "Invalid datetime interval: start must be before end"
                                    .to_string(),
                            }
                            .build());
                        }
                    }
                    datetime_clauses.push(format!("i._datetime <= '{}'", parts[1]));
                }
            } else {
                parse_rfc3339(datetime)?;
                datetime_clauses.push(format!("i._datetime = '{}'", datetime));
            }
        }

        let where_clause = where_clauses.join(" AND ");
        let datetime_clause = if datetime_clauses.is_empty() {
            String::new()
        } else {
            format!(" AND {}", datetime_clauses.join(" AND "))
        };

        // Early return if no matching collections found
        if tables.is_empty() {
            return Ok(StacSearchResult {
                returned: 0,
                matched: Some(0),
                items: vec![],
            });
        }

        // Count query - sum counts from all tables
        let count_sql = format!(
            "SELECT COALESCE(SUM(cnt), 0)::bigint FROM ({}) sub",
            tables
                .iter()
                .map(|(_, table, has_datetime)| {
                    let dt = if *has_datetime { datetime_clause.as_str() } else { "" };
                    format!("SELECT COUNT(*) as cnt FROM {table} i WHERE {where_clause}{dt}")
                })
                .join(" UNION ALL ")
        );
        let count: (i64,) = sqlx::query_as(&count_sql).fetch_one(self.db.pool()).await?;

        // Build the system columns exclusion list for to_jsonb
        let system_exclusions = crate::services::collection_service::SYSTEM_COLUMNS
            .iter()
            .map(|c| format!("- '{}'", c))
            .collect::<Vec<_>>()
            .join(" ");

        // Data query - get items (wrap each SELECT in parens for valid UNION ALL with ORDER BY)
        let sql = format!(
            "{} LIMIT {}",
            tables
                .iter()
                .map(|(collection, table, has_datetime)| {
                    let datetime_col = if *has_datetime {
                        "i._datetime"
                    } else {
                        "i._created_at"
                    };
                    let order_col = if *has_datetime {
                        "i._datetime DESC NULLS LAST"
                    } else {
                        "i._id"
                    };
                    let dt = if *has_datetime { datetime_clause.as_str() } else { "" };
                    format!(
                        r#"(SELECT
                i._id,
                '{collection}' as collection_name,
                ST_AsGeoJSON(i.geometry)::jsonb as geometry,
                ST_XMin(i.geometry) as minx,
                ST_YMin(i.geometry) as miny,
                ST_XMax(i.geometry) as maxx,
                ST_YMax(i.geometry) as maxy,
                {datetime_col} as _datetime,
                (to_jsonb(i.*) {system_exclusions}) as properties
            FROM {table} i
            WHERE {where_clause}{dt}
            ORDER BY {order_col})"#
                    )
                })
                .join(" UNION ALL "),
            params.limit
        );

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

        let assets_map: HashMap<_, _> = try_join_all(grouped.into_iter().map(
            |(collection_name, ids)| async move {
                let collection = self
                    .collection_service
                    .get_collection(username, &collection_name)
                    .await?
                    .as_collection();
                let assets = if self.collection_service.has_assets(&collection).await? {
                    self.feature_service
                        .get_assets_for_items(&collection, &ids)
                        .await?
                } else {
                    HashMap::new()
                };
                AppResult::<(String, HashMap<Uuid, Assets>)>::Ok((collection_name, assets))
            },
        ))
        .await?
        .into_iter()
        .collect();

        let items: Vec<StacItem> = rows
            .into_iter()
            .map(
                |(id, collection, geometry, minx, miny, maxx, maxy, datetime, properties)| {
                    let item_assets = assets_map
                        .get(&collection)
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
                            Link::new(&self.base_url, rel::ROOT)
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

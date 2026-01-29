use aide::{
    axum::{
        routing::{get_with, post_with},
        ApiRouter,
    },
    transform::TransformOperation,
};
use axum::{
    extract::{Extension, Query, State},
    Json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::common::{media_type, rel, Link};
use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::error::AppResult;
use crate::services::StacService;

/// STAC Item (extends GeoJSON Feature)
#[derive(Debug, Serialize, JsonSchema)]
pub struct StacItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub stac_version: String,
    pub stac_extensions: Vec<String>,
    pub id: String,
    pub geometry: serde_json::Value,
    pub bbox: Option<Vec<f64>>,
    pub properties: StacItemProperties,
    pub links: Vec<Link>,
    pub assets: serde_json::Value,
    pub collection: String,
}

/// STAC Item properties
#[derive(Debug, Serialize, JsonSchema)]
pub struct StacItemProperties {
    pub datetime: Option<String>,
    #[serde(flatten)]
    pub additional: serde_json::Value,
}

/// STAC search parameters
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct StacSearchParams {
    /// Bounding box: minx,miny,maxx,maxy
    pub bbox: Option<String>,

    /// Datetime or interval
    pub datetime: Option<String>,

    /// Collection IDs (comma-separated)
    pub collections: Option<String>,

    /// Item IDs (comma-separated)
    pub ids: Option<String>,

    /// Maximum items to return
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Pagination token
    pub token: Option<String>,

    /// Intersects geometry (GeoJSON)
    pub intersects: Option<String>,
}

fn default_limit() -> u32 {
    10
}

/// STAC search POST body
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StacSearchBody {
    #[serde(flatten)]
    pub params: StacSearchParams,
    /// Intersects geometry as GeoJSON object
    pub intersects: Option<serde_json::Value>,
}

/// STAC ItemCollection (search result)
#[derive(Debug, Serialize, JsonSchema)]
pub struct StacItemCollection {
    #[serde(rename = "type")]
    pub collection_type: String,
    pub features: Vec<StacItem>,
    pub links: Vec<Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<StacContext>,
}

/// STAC search context
#[derive(Debug, Serialize, JsonSchema)]
pub struct StacContext {
    pub returned: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched: Option<u64>,
    pub limit: u32,
}

/// STAC search (GET)
pub async fn search_get(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<StacService>>,
    Query(params): Query<StacSearchParams>,
) -> AppResult<Json<StacItemCollection>> {
    let results = service.search(&user.username, &params).await?;

    let base_url = &config.base_url;

    let response = StacItemCollection {
        collection_type: "FeatureCollection".to_string(),
        features: results.items,
        links: vec![
            Link::new(format!("{}/stac/search", base_url), rel::SELF)
                .with_type(media_type::GEOJSON),
            Link::new(format!("{}/stac", base_url), rel::ROOT)
                .with_type(media_type::JSON),
        ],
        context: Some(StacContext {
            returned: results.returned,
            matched: results.matched,
            limit: params.limit,
        }),
    };

    Ok(Json(response))
}

fn search_get_docs(op: TransformOperation) -> TransformOperation {
    op.summary("STAC search (GET)")
        .description("Search for STAC items using query parameters")
        .tag("STAC")
        .response_with::<200, Json<StacItemCollection>, _>(|res| {
            res.description("STAC item search results")
        })
}

/// STAC search (POST)
pub async fn search_post(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<StacService>>,
    Json(body): Json<StacSearchBody>,
) -> AppResult<Json<StacItemCollection>> {
    let results = service.search(&user.username, &body.params).await?;

    let base_url = &config.base_url;

    let response = StacItemCollection {
        collection_type: "FeatureCollection".to_string(),
        features: results.items,
        links: vec![
            Link::new(format!("{}/stac/search", base_url), rel::SELF)
                .with_type(media_type::GEOJSON),
            Link::new(format!("{}/stac", base_url), rel::ROOT)
                .with_type(media_type::JSON),
        ],
        context: Some(StacContext {
            returned: results.returned,
            matched: results.matched,
            limit: body.params.limit,
        }),
    };

    Ok(Json(response))
}

fn search_post_docs(op: TransformOperation) -> TransformOperation {
    op.summary("STAC search (POST)")
        .description("Search for STAC items using a JSON request body with spatial/temporal filters")
        .tag("STAC")
        .response_with::<200, Json<StacItemCollection>, _>(|res| {
            res.description("STAC item search results")
        })
}

pub fn routes(service: Arc<StacService>) -> ApiRouter {
    ApiRouter::new()
        .api_route(
            "/stac/search",
            get_with(search_get, search_get_docs)
                .post_with(search_post, search_post_docs),
        )
        .with_state(service)
}

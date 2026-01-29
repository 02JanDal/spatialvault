use aide::{
    axum::{
        routing::get_with,
        ApiRouter,
    },
    transform::TransformOperation,
};
use axum::{extract::Extension, Json};
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

use crate::api::common::{media_type, rel, Link};
use crate::config::Config;

/// STAC Catalog root
#[derive(Debug, Serialize, JsonSchema)]
pub struct StacCatalog {
    #[serde(rename = "type")]
    pub catalog_type: String,
    pub stac_version: String,
    pub stac_extensions: Vec<String>,
    pub id: String,
    pub title: String,
    pub description: String,
    pub links: Vec<Link>,
    #[serde(rename = "conformsTo")]
    pub conforms_to: Vec<String>,
}

/// Get STAC catalog root
pub async fn get_catalog(Extension(config): Extension<Arc<Config>>) -> Json<StacCatalog> {
    let base_url = &config.base_url;

    let catalog = StacCatalog {
        catalog_type: "Catalog".to_string(),
        stac_version: "1.0.0".to_string(),
        stac_extensions: vec![],
        id: "spatialvault".to_string(),
        title: "SpatialVault STAC Catalog".to_string(),
        description: "STAC catalog for SpatialVault geospatial data".to_string(),
        links: vec![
            Link::new(format!("{}/stac", base_url), rel::SELF)
                .with_type(media_type::JSON),
            Link::new(format!("{}/stac", base_url), rel::ROOT)
                .with_type(media_type::JSON),
            Link::new(base_url, "parent")
                .with_type(media_type::JSON)
                .with_title("API Landing Page"),
            Link::new(format!("{}/collections", base_url), rel::DATA)
                .with_type(media_type::JSON)
                .with_title("Collections"),
            Link::new(format!("{}/stac/search", base_url), "search")
                .with_type(media_type::GEOJSON)
                .with_title("STAC Search"),
            Link::new(format!("{}/api", base_url), rel::SERVICE_DESC)
                .with_type(media_type::OPENAPI_JSON)
                .with_title("OpenAPI definition"),
            Link::new(format!("{}/conformance", base_url), rel::CONFORMANCE)
                .with_type(media_type::JSON)
                .with_title("Conformance"),
        ],
        conforms_to: vec![
            "https://api.stacspec.org/v1.0.0/core".to_string(),
            "https://api.stacspec.org/v1.0.0/item-search".to_string(),
            "https://api.stacspec.org/v1.0.0/ogcapi-features".to_string(),
            "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/core".to_string(),
            "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/geojson".to_string(),
        ],
    };

    Json(catalog)
}

fn get_catalog_docs(op: TransformOperation) -> TransformOperation {
    op.summary("STAC Catalog root")
        .description("Returns the root STAC Catalog with links to collections and search")
        .tag("STAC")
        .response_with::<200, Json<StacCatalog>, _>(|res| {
            res.description("STAC Catalog root document")
        })
}

pub fn routes() -> ApiRouter {
    ApiRouter::new().api_route("/stac", get_with(get_catalog, get_catalog_docs))
}

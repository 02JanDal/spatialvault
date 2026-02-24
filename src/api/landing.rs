use aide::{
    OperationIo,
    axum::{ApiRouter, routing::get_with},
    transform::TransformOperation,
};
use axum::{Extension, Json};
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

use super::common::{Link, media_type, rel};
use super::conformance;
use crate::config::Config;

/// OGC API Landing Page / STAC Catalog root
#[derive(Debug, Serialize, JsonSchema, OperationIo)]
#[aide(output)]
#[serde(rename_all = "camelCase")]
pub struct LandingPage {
    #[serde(rename = "type")]
    pub catalog_type: String,
    pub id: String,
    pub title: String,
    pub description: String,
    pub stac_version: String,
    pub stac_extensions: Vec<String>,
    pub conforms_to: Vec<String>,
    pub links: Vec<Link>,
}

async fn get_landing_page(Extension(config): Extension<Arc<Config>>) -> Json<LandingPage> {
    let base_url = &config.base_url;

    let landing = LandingPage {
        catalog_type: "Catalog".to_string(),
        id: "spatialvault".to_string(),
        title: "SpatialVault".to_string(),
        description: "OGC API compliant geospatial data service with STAC integration".to_string(),
        stac_version: "1.0.0".to_string(),
        stac_extensions: vec![],
        conforms_to: conformance::conforms_to(),
        links: vec![
            Link::new(base_url, rel::SELF)
                .with_type(media_type::JSON)
                .with_title("This document"),
            Link::new(base_url, rel::ROOT)
                .with_type(media_type::JSON)
                .with_title("Root catalog"),
            Link::new(format!("{}/api", base_url), rel::SERVICE_DESC)
                .with_type(media_type::OPENAPI_JSON)
                .with_title("OpenAPI definition"),
            Link::new(format!("{}/conformance", base_url), rel::CONFORMANCE)
                .with_type(media_type::JSON)
                .with_title("Conformance declaration"),
            Link::new(format!("{}/collections", base_url), rel::DATA)
                .with_type(media_type::JSON)
                .with_title("Collections"),
            Link::new(format!("{}/search", base_url), "search")
                .with_type(media_type::GEOJSON)
                .with_title("STAC Search"),
            Link::new(
                format!("{}/processes", base_url),
                "http://www.opengis.net/def/rel/ogc/1.0/processes",
            )
            .with_type(media_type::JSON)
            .with_title("Processes"),
            Link::new(
                format!("{}/jobs", base_url),
                "http://www.opengis.net/def/rel/ogc/1.0/job-list",
            )
            .with_type(media_type::JSON)
            .with_title("Jobs"),
        ],
    };

    Json(landing)
}

fn get_landing_page_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Landing page")
        .description("Returns the landing page / STAC catalog root with links to the API capabilities")
        .tag("Core")
        .response_with::<200, Json<LandingPage>, _>(|res| res.description("Landing page response"))
}

pub fn routes() -> ApiRouter {
    ApiRouter::new().api_route("/", get_with(get_landing_page, get_landing_page_docs))
}

use aide::{
    axum::{routing::get_with, ApiRouter},
    transform::TransformOperation,
    OperationIo,
};
use axum::{Extension, Json};
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

use super::common::{media_type, rel, Link};
use crate::config::Config;

/// OGC API Landing Page response
#[derive(Debug, Serialize, JsonSchema, OperationIo)]
#[aide(output)]
pub struct LandingPage {
    pub title: String,
    pub description: String,
    pub links: Vec<Link>,
}

async fn get_landing_page(Extension(config): Extension<Arc<Config>>) -> Json<LandingPage> {
    let base_url = &config.base_url;

    let landing = LandingPage {
        title: "SpatialVault".to_string(),
        description: "OGC API compliant geospatial data service with STAC integration".to_string(),
        links: vec![
            Link::new(base_url, rel::SELF)
                .with_type(media_type::JSON)
                .with_title("This document"),
            Link::new(format!("{}/api", base_url), rel::SERVICE_DESC)
                .with_type(media_type::OPENAPI_JSON)
                .with_title("OpenAPI definition"),
            Link::new(format!("{}/conformance", base_url), rel::CONFORMANCE)
                .with_type(media_type::JSON)
                .with_title("Conformance declaration"),
            Link::new(format!("{}/collections", base_url), rel::DATA)
                .with_type(media_type::JSON)
                .with_title("Collections"),
            Link::new(format!("{}/stac", base_url), rel::ROOT)
                .with_type(media_type::JSON)
                .with_title("STAC Catalog"),
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
        .description("Returns the landing page with links to the API capabilities")
        .tag("Core")
        .response_with::<200, Json<LandingPage>, _>(|res| res.description("Landing page response"))
}

pub fn routes() -> ApiRouter {
    ApiRouter::new().api_route("/", get_with(get_landing_page, get_landing_page_docs))
}

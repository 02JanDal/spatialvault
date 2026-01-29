use aide::{
    axum::{routing::get_with, ApiRouter},
    transform::TransformOperation,
    OperationIo,
};
use axum::Json;
use schemars::JsonSchema;
use serde::Serialize;

/// OGC API Conformance declaration
#[derive(Debug, Serialize, JsonSchema, OperationIo)]
#[aide(output)]
#[serde(rename_all = "camelCase")]
pub struct Conformance {
    pub conforms_to: Vec<String>,
}

/// Conformance class URIs
pub mod classes {
    // OGC API Common
    pub const COMMON_CORE: &str = "http://www.opengis.net/spec/ogcapi-common-1/1.0/conf/core";
    pub const COMMON_LANDING: &str =
        "http://www.opengis.net/spec/ogcapi-common-1/1.0/conf/landing-page";
    pub const COMMON_JSON: &str = "http://www.opengis.net/spec/ogcapi-common-1/1.0/conf/json";
    pub const COMMON_OAS30: &str = "http://www.opengis.net/spec/ogcapi-common-1/1.0/conf/oas30";

    // OGC API Features
    pub const FEATURES_CORE: &str = "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/core";
    pub const FEATURES_GEOJSON: &str =
        "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/geojson";
    pub const FEATURES_OAS30: &str = "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/oas30";
    pub const FEATURES_CRS: &str = "http://www.opengis.net/spec/ogcapi-features-2/1.0/conf/crs";

    // OGC API Tiles
    pub const TILES_CORE: &str = "http://www.opengis.net/spec/ogcapi-tiles-1/1.0/conf/core";
    pub const TILES_TILESET: &str = "http://www.opengis.net/spec/ogcapi-tiles-1/1.0/conf/tileset";

    // OGC API Coverages
    pub const COVERAGES_CORE: &str =
        "http://www.opengis.net/spec/ogcapi-coverages-1/0.0/conf/core";
    pub const COVERAGES_GEOTIFF: &str =
        "http://www.opengis.net/spec/ogcapi-coverages-1/0.0/conf/geotiff";

    // OGC API Processes
    pub const PROCESSES_CORE: &str =
        "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/core";
    pub const PROCESSES_JSON: &str =
        "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/json";
    pub const PROCESSES_OGC_PROCESS: &str =
        "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/ogc-process-description";
    pub const PROCESSES_JOB_LIST: &str =
        "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/job-list";
    pub const PROCESSES_DISMISS: &str =
        "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/dismiss";

    // STAC Core
    pub const STAC_CORE: &str = "https://api.stacspec.org/v1.0.0/core";
    pub const STAC_ITEM_SEARCH: &str = "https://api.stacspec.org/v1.0.0/item-search";
    pub const STAC_COLLECTIONS: &str = "https://api.stacspec.org/v1.0.0/collections";
    pub const STAC_FEATURES: &str = "https://api.stacspec.org/v1.0.0/ogcapi-features";

    // STAC Transaction Extensions
    pub const STAC_COLLECTION_TRANSACTION: &str =
        "https://api.stacspec.org/v1.0.0/collections/extensions/transaction";
    pub const STAC_ITEM_TRANSACTION: &str =
        "https://api.stacspec.org/v1.0.0/ogcapi-features/extensions/transaction";

    // CQL2 Filtering
    pub const CQL2_TEXT: &str = "http://www.opengis.net/spec/cql2/1.0/conf/cql2-text";
    pub const CQL2_JSON: &str = "http://www.opengis.net/spec/cql2/1.0/conf/cql2-json";
}

async fn get_conformance() -> Json<Conformance> {
    let conformance = Conformance {
        conforms_to: vec![
            // OGC API Common
            classes::COMMON_CORE.to_string(),
            classes::COMMON_LANDING.to_string(),
            classes::COMMON_JSON.to_string(),
            classes::COMMON_OAS30.to_string(),
            // OGC API Features
            classes::FEATURES_CORE.to_string(),
            classes::FEATURES_GEOJSON.to_string(),
            classes::FEATURES_OAS30.to_string(),
            classes::FEATURES_CRS.to_string(),
            // OGC API Tiles
            classes::TILES_CORE.to_string(),
            classes::TILES_TILESET.to_string(),
            // OGC API Coverages
            classes::COVERAGES_CORE.to_string(),
            classes::COVERAGES_GEOTIFF.to_string(),
            // OGC API Processes
            classes::PROCESSES_CORE.to_string(),
            classes::PROCESSES_JSON.to_string(),
            classes::PROCESSES_OGC_PROCESS.to_string(),
            classes::PROCESSES_JOB_LIST.to_string(),
            classes::PROCESSES_DISMISS.to_string(),
            // STAC Core
            classes::STAC_CORE.to_string(),
            classes::STAC_COLLECTIONS.to_string(),
            classes::STAC_FEATURES.to_string(),
            classes::STAC_ITEM_SEARCH.to_string(),
            // STAC Transaction Extensions
            classes::STAC_COLLECTION_TRANSACTION.to_string(),
            classes::STAC_ITEM_TRANSACTION.to_string(),
            // CQL2
            classes::CQL2_TEXT.to_string(),
            classes::CQL2_JSON.to_string(),
        ],
    };

    Json(conformance)
}

fn get_conformance_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Conformance declaration")
        .description("Returns the list of conformance classes that this API implements")
        .tag("Core")
        .response_with::<200, Json<Conformance>, _>(|res| {
            res.description("Conformance declaration response")
        })
}

pub fn routes() -> ApiRouter {
    ApiRouter::new().api_route("/conformance", get_with(get_conformance, get_conformance_docs))
}

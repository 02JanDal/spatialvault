use crate::api::common::{Link, SpatialExtent, media_type, rel};
use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::error::{AppError, BadRequest};
use crate::services::{CollectionService, CoverageService};
use aide::{
    axum::{ApiRouter, routing::get_with},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Extension, State},
    response::{IntoResponse, Response},
};
use axum_extra::routing::TypedPath;
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

/// Coverage description (OGC API Coverages)
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CoverageDescription {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extent: Option<SpatialExtent>,
    pub crs: Vec<String>,
    pub links: Vec<Link>,
}

/// Domain set (spatial/temporal extent and resolution)
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DomainSet {
    #[serde(rename = "type")]
    pub domain_type: String,
    pub general_grid: GeneralGrid,
}

/// General grid description
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GeneralGrid {
    #[serde(rename = "type")]
    pub grid_type: String,
    pub srs_name: String,
    pub axis_labels: Vec<String>,
    pub axis: Vec<GridAxis>,
}

/// Grid axis description
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GridAxis {
    #[serde(rename = "type")]
    pub axis_type: String,
    pub axis_label: String,
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub resolution: f64,
    pub uom_label: String,
}

/// Range type (band/channel descriptions)
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RangeType {
    #[serde(rename = "type")]
    pub range_type: String,
    pub field: Vec<RangeField>,
}

/// Range field (band description)
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RangeField {
    #[serde(rename = "type")]
    pub field_type: String,
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub definition: String,
    pub uom: UnitOfMeasure,
}

/// Unit of measure
#[derive(Debug, Serialize, JsonSchema)]
pub struct UnitOfMeasure {
    #[serde(rename = "type")]
    pub uom_type: String,
    pub code: String,
}

/// Path parameters for coverage endpoint
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/coverage")]
pub struct CoveragePath {
    /// The collection identifier
    pub collection_id: String,
}

/// Get coverage description
pub async fn get_coverage(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((_service, collection_service)): State<(Arc<CoverageService>, Arc<CollectionService>)>,
    path: CoveragePath,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    let collection = collection_service
        .get_collection(&user.username, &collection_id)
        .await?;

    // Verify this is a raster collection
    if collection.collection_type != "raster" {
        return Err(BadRequest {
            message: "Coverage endpoint only available for raster collections".to_string(),
        }
        .build());
    }

    let base_url = &config.base_url;

    let coverage = CoverageDescription {
        id: collection.canonical_name.clone(),
        title: collection.title.clone(),
        description: collection.description.clone(),
        extent: None, // Computed on demand
        crs: vec![
            "http://www.opengis.net/def/crs/OGC/1.3/CRS84".to_string(),
            "http://www.opengis.net/def/crs/EPSG/0/4326".to_string(),
        ],
        links: vec![
            Link::new(
                format!("{}/collections/{}/coverage", base_url, collection_id),
                rel::SELF,
            )
            .with_type(media_type::JSON),
            Link::new(
                format!(
                    "{}/collections/{}/coverage/domainset",
                    base_url, collection_id
                ),
                "domainset",
            )
            .with_type(media_type::JSON),
            Link::new(
                format!(
                    "{}/collections/{}/coverage/rangetype",
                    base_url, collection_id
                ),
                "rangetype",
            )
            .with_type(media_type::JSON),
            Link::new(
                format!("{}/collections/{}", base_url, collection_id),
                rel::COLLECTION,
            )
            .with_type(media_type::JSON),
        ],
    };

    Ok(Json(coverage).into_response())
}

fn get_coverage_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get coverage description")
        .description("Returns the coverage description for a raster collection")
        .tag("Coverages")
        .response_with::<200, Json<CoverageDescription>, _>(|res| {
            res.description("Coverage description")
        })
        .response_with::<400, (), _>(|res| res.description("Not a raster collection"))
        .response_with::<404, (), _>(|res| res.description("Collection not found"))
}

/// Path parameters for coverage domainset endpoint
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/coverage/domainset")]
pub struct CoverageDomainsetPath {
    /// The collection identifier
    pub collection_id: String,
}

/// Get domain set
pub async fn get_domainset(
    Extension(user): Extension<AuthenticatedUser>,
    State((service, collection_service)): State<(Arc<CoverageService>, Arc<CollectionService>)>,
    path: CoverageDomainsetPath,
) -> Result<Response, AppError> {
    let collection = collection_service
        .get_collection(&user.username, &path.collection_id)
        .await?;

    let domain = service
        .get_domainset(&user.username, &collection.as_collection())
        .await?;

    Ok(Json(domain).into_response())
}

fn get_domainset_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get domain set")
        .description("Returns the spatial/temporal extent and resolution of a coverage")
        .tag("Coverages")
        .response_with::<200, Json<DomainSet>, _>(|res| res.description("Domain set description"))
}

/// Path parameters for coverage rangetype endpoint
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/coverage/rangetype")]
pub struct CoverageRangetypePath {
    /// The collection identifier
    pub collection_id: String,
}

/// Get range type
pub async fn get_rangetype(
    Extension(user): Extension<AuthenticatedUser>,
    State((service, collection_service)): State<(Arc<CoverageService>, Arc<CollectionService>)>,
    path: CoverageRangetypePath,
) -> Result<Response, AppError> {
    let collection = collection_service
        .get_collection(&user.username, &path.collection_id)
        .await?;

    let rangetype = service
        .get_rangetype(&user.username, &collection.as_collection())
        .await?;

    Ok(Json(rangetype).into_response())
}

fn get_rangetype_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get range type")
        .description("Returns the band/channel descriptions of a coverage")
        .tag("Coverages")
        .response_with::<200, Json<RangeType>, _>(|res| res.description("Range type description"))
}

pub fn routes(
    service: Arc<CoverageService>,
    collection_service: Arc<CollectionService>,
) -> ApiRouter {
    ApiRouter::new()
        .api_route(
            CoveragePath::PATH,
            get_with(get_coverage, get_coverage_docs),
        )
        .api_route(
            CoverageDomainsetPath::PATH,
            get_with(get_domainset, get_domainset_docs),
        )
        .api_route(
            CoverageRangetypePath::PATH,
            get_with(get_rangetype, get_rangetype_docs),
        )
        .with_state((service, collection_service))
}

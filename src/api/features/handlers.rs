use super::crs::{ContentCrs, parse_crs_param};
use super::query::FeatureQueryParams;
use super::schemas::{CreateFeaturePayload, UpdateFeaturePayload, resolve_asset_uploads};
use crate::api::common;
use crate::api::common::Location;
use crate::api::common::{Assets, GeoJsonGeometry, Link, etag, media_type, rel};
use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::error::{AppError, NotFound};
use crate::services::FeatureService;
use crate::storage::S3Storage;
use aide::{
    axum::{ApiRouter, routing::get_with},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_extra::TypedHeader;
use axum_extra::headers::{ContentType, IfMatch};
use axum_extra::routing::TypedPath;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snafu::OptionExt;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

type FeatureState = (Arc<S3Storage>, Arc<FeatureService>);

/// GeoJSON Feature (also serves as STAC Item for raster/pointcloud collections)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Feature {
    #[serde(rename = "type")]
    pub feature_type: String,
    pub id: String,
    pub geometry: GeoJsonGeometry,
    pub properties: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<Link>>,
    /// STAC fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<Assets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stac_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stac_extensions: Option<Vec<String>>,
}

/// GeoJSON FeatureCollection
#[derive(Debug, Serialize, JsonSchema)]
pub struct FeatureCollection {
    #[serde(rename = "type")]
    pub feature_type: String,
    pub features: Vec<Feature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<Link>>,
    #[serde(rename = "numberMatched", skip_serializing_if = "Option::is_none")]
    pub number_matched: Option<u64>,
    #[serde(rename = "numberReturned")]
    pub number_returned: u64,
    #[serde(rename = "timeStamp", skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// Request to create a feature or STAC item
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateFeatureRequest {
    #[serde(rename = "type")]
    pub feature_type: Option<String>,
    pub geometry: GeoJsonGeometry,
    pub properties: serde_json::Value,
    /// STAC item assets
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<Assets>,
}

/// Request to update a feature or STAC item (PATCH - JSON Merge Patch)
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateFeatureRequest {
    pub geometry: Option<GeoJsonGeometry>,
    pub properties: Option<serde_json::Value>,
    /// STAC item assets
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<Assets>,
}

/// Path parameters for collection items endpoints
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/items")]
pub struct CollectionItemsPath {
    /// The collection identifier
    pub collection_id: String,
}

pub async fn list_features(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((_storage, service)): State<FeatureState>,
    path: CollectionItemsPath,
    Query(params): Query<FeatureQueryParams>,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;

    params.validate()?;

    let target_crs = parse_crs_param(params.crs.as_deref())?;
    let bbox_crs = parse_crs_param(params.bbox_crs.as_deref())?;

    let (features, total_count, storage_srid) = service
        .list_features(
            &user.username,
            &collection_id,
            params.limit,
            params.offset,
            params.bbox.as_deref(),
            bbox_crs,
            target_crs,
            params.datetime.as_deref(),
            params.filter.as_deref(),
            params.filter_lang.as_deref(),
        )
        .await?;

    let base_url = &config.base_url;
    let response_crs = target_crs.unwrap_or(storage_srid);

    // Build pagination links
    let mut links = vec![
        Link::new(
            format!("{}/collections/{}/items", base_url, collection_id),
            rel::SELF,
        )
        .with_type(media_type::GEOJSON),
        Link::new(
            format!("{}/collections/{}", base_url, collection_id),
            rel::COLLECTION,
        )
        .with_type(media_type::JSON),
    ];

    // Add next/prev links if needed
    if params.offset + params.limit < total_count as u32 {
        links.push(
            Link::new(
                format!(
                    "{}/collections/{}/items?offset={}&limit={}",
                    base_url,
                    collection_id,
                    params.offset + params.limit,
                    params.limit
                ),
                rel::NEXT,
            )
            .with_type(media_type::GEOJSON),
        );
    }

    if params.offset > 0 {
        let prev_offset = params.offset.saturating_sub(params.limit);
        links.push(
            Link::new(
                format!(
                    "{}/collections/{}/items?offset={}&limit={}",
                    base_url, collection_id, prev_offset, params.limit
                ),
                rel::PREV,
            )
            .with_type(media_type::GEOJSON),
        );
    }

    let collection = FeatureCollection {
        feature_type: "FeatureCollection".to_string(),
        number_matched: Some(total_count as u64),
        number_returned: features.len() as u64,
        features,
        links: Some(links),
        timestamp: Some(chrono::Utc::now().to_rfc3339()),
    };

    Ok((
        TypedHeader(ContentType::from_str(media_type::GEOJSON).unwrap()),
        TypedHeader(ContentCrs(response_crs)),
        Json(collection),
    )
        .into_response())
}

fn list_features_docs(op: TransformOperation) -> TransformOperation {
    op.summary("List features")
        .description("Returns a paginated list of features in a collection, with optional spatial, temporal, and CQL filtering")
        .tag("Features")
        .response_with::<200, Json<FeatureCollection>, _>(|res| {
            res.description("List of features")
        })
}

/// Path parameters for single feature endpoints
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/items/{feature_id}")]
pub struct FeaturePath {
    /// The collection identifier
    pub collection_id: String,
    /// The feature UUID
    pub feature_id: Uuid,
}

pub async fn get_feature(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((_storage, service)): State<FeatureState>,
    path: FeaturePath,
    Query(params): Query<FeatureQueryParams>,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    let feature_id = path.feature_id;
    let target_crs = parse_crs_param(params.crs.as_deref())?;

    let (feature, version, storage_srid) = service
        .get_feature(&user.username, &collection_id, feature_id, target_crs)
        .await?
        .context(NotFound {
            message: format!(
                "Feature {} not found in collection {}",
                feature_id, collection_id
            ),
        })?;

    let base_url = &config.base_url;
    let response_crs = target_crs.unwrap_or(storage_srid);

    let mut feature = feature;
    feature.links = Some(vec![
        Link::new(
            format!(
                "{}/collections/{}/items/{}",
                base_url, collection_id, feature_id
            ),
            rel::SELF,
        )
        .with_type(media_type::GEOJSON),
        Link::new(
            format!("{}/collections/{}", base_url, collection_id),
            rel::COLLECTION,
        )
        .with_type(media_type::JSON),
    ]);

    Ok((
        TypedHeader(ContentType::from_str(media_type::GEOJSON).unwrap()),
        TypedHeader(ContentCrs(response_crs)),
        TypedHeader(etag::make(version)),
        Json(feature),
    )
        .into_response())
}

fn get_feature_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get feature")
        .description("Returns a single feature by ID from a collection")
        .tag("Features")
        .response_with::<200, Json<Feature>, _>(|res| res.description("Feature details"))
        .response_with::<404, (), _>(|res| res.description("Feature not found"))
}

pub async fn create_feature(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((storage, service)): State<FeatureState>,
    path: CollectionItemsPath,
    payload: CreateFeaturePayload,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;

    let (mut request, files) = match payload {
        CreateFeaturePayload::Json(r) => (r, HashMap::new()),
        CreateFeaturePayload::Multipart { item, files } => (item, files),
    };
    resolve_asset_uploads(&mut request.assets, files, &storage).await?;

    // Extract datetime from properties if present
    let datetime = request
        .properties
        .get("datetime")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let (feature, version) = service
        .create_feature(
            &user.username,
            &collection_id,
            &request.geometry,
            &request.properties,
            datetime,
            request.assets,
        )
        .await?;

    let base_url = &config.base_url;

    Ok((
        StatusCode::CREATED,
        TypedHeader(ContentType::from_str(media_type::GEOJSON).unwrap()),
        TypedHeader(Location(format!(
            "{}/collections/{}/items/{}",
            base_url, collection_id, feature.id
        ))),
        TypedHeader(etag::make(version)),
        Json(feature),
    )
        .into_response())
}

fn create_feature_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create feature")
        .description(
            "Creates a new feature in a collection. Supports both vector features and STAC items.",
        )
        .tag("Features")
        .response_with::<201, Json<Feature>, _>(|res| {
            res.description("Feature created successfully")
        })
        .response_with::<400, (), _>(|res| res.description("Invalid request"))
}

pub async fn update_feature(
    Extension(user): Extension<AuthenticatedUser>,
    State((storage, service)): State<FeatureState>,
    path: FeaturePath,
    if_match: Option<TypedHeader<IfMatch>>,
    headers: axum::http::HeaderMap,
    payload: UpdateFeaturePayload,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;

    let feature_id = path.feature_id;

    let (mut request, files) = match payload {
        UpdateFeaturePayload::Json(r) => (r, HashMap::new()),
        UpdateFeaturePayload::Multipart { item, files } => (item, files),
    };
    resolve_asset_uploads(&mut request.assets, files, &storage).await?;

    // Extract datetime from properties if present
    let datetime = request
        .properties
        .as_ref()
        .and_then(|p| p.get("datetime"))
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let (feature, new_version) = service
        .update_feature(
            &user.username,
            &collection_id,
            feature_id,
            &common::fix_header(headers, if_match),
            request.geometry,
            request.properties,
            datetime,
            request.assets,
        )
        .await?;

    Ok((TypedHeader(etag::make(new_version)), Json(feature)).into_response())
}

fn update_feature_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Update feature (partial)")
        .description("Partially updates a feature using JSON Merge Patch. If-Match header is optional; when provided, enables optimistic locking.")
        .tag("Features")
        .response_with::<200, Json<Feature>, _>(|res| {
            res.description("Feature updated successfully")
        })
        .response_with::<404, (), _>(|res| res.description("Feature not found"))
        .response_with::<412, (), _>(|res| res.description("Precondition failed (ETag mismatch)"))
}

pub async fn replace_feature(
    Extension(user): Extension<AuthenticatedUser>,
    State((storage, service)): State<FeatureState>,
    path: FeaturePath,
    if_match: Option<TypedHeader<IfMatch>>,
    headers: axum::http::HeaderMap,
    payload: CreateFeaturePayload,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    let feature_id = path.feature_id;

    let (mut request, files) = match payload {
        CreateFeaturePayload::Json(r) => (r, HashMap::new()),
        CreateFeaturePayload::Multipart { item, files } => (item, files),
    };
    resolve_asset_uploads(&mut request.assets, files, &storage).await?;

    // Extract datetime from properties if present
    let datetime = request
        .properties
        .get("datetime")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let (feature, new_version) = service
        .replace_feature(
            &user.username,
            &collection_id,
            feature_id,
            &common::fix_header(headers, if_match),
            request.geometry,
            request.properties,
            datetime,
            request.assets,
        )
        .await?;

    Ok((TypedHeader(etag::make(new_version)), Json(feature)).into_response())
}

fn replace_feature_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Replace feature")
        .description("Fully replaces a feature in a collection. If-Match header is optional; when provided, enables optimistic locking.")
        .tag("Features")
        .response_with::<200, Json<Feature>, _>(|res| {
            res.description("Feature replaced successfully")
        })
        .response_with::<404, (), _>(|res| res.description("Feature not found"))
        .response_with::<412, (), _>(|res| res.description("Precondition failed (ETag mismatch)"))
}

pub async fn delete_feature(
    Extension(user): Extension<AuthenticatedUser>,
    State((_storage, service)): State<FeatureState>,
    path: FeaturePath,
    if_match: Option<TypedHeader<IfMatch>>,
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    let feature_id = path.feature_id;

    service
        .delete_feature(
            &user.username,
            &collection_id,
            feature_id,
            &common::fix_header(headers, if_match),
        )
        .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

fn delete_feature_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Delete feature")
        .description("Deletes a feature from a collection. If-Match header is optional; when provided, enables optimistic locking.")
        .tag("Features")
        .response_with::<204, (), _>(|res| res.description("Feature deleted"))
        .response_with::<404, (), _>(|res| res.description("Feature not found"))
        .response_with::<412, (), _>(|res| res.description("Precondition failed (ETag mismatch)"))
}

pub fn routes(storage: Arc<S3Storage>, service: Arc<FeatureService>) -> ApiRouter {
    ApiRouter::new()
        .api_route(
            CollectionItemsPath::PATH,
            get_with(list_features, list_features_docs)
                .post_with(create_feature, create_feature_docs),
        )
        .api_route(
            FeaturePath::PATH,
            get_with(get_feature, get_feature_docs)
                .put_with(replace_feature, replace_feature_docs)
                .patch_with(update_feature, update_feature_docs)
                .delete_with(delete_feature, delete_feature_docs),
        )
        .with_state((storage, service))
}

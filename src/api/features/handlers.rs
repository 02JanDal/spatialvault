use aide::{
    axum::{
        routing::{delete_with, get_with, patch_with, post_with, put_with},
        ApiRouter,
    },
    transform::TransformOperation,
};
use axum::{
    extract::{Extension, Query, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::crs::{content_crs_header, parse_crs_param};
use super::query::FeatureQueryParams;
use crate::api::common::{media_type, rel, Link};
use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::services::{CollectionService, FeatureService};

/// GeoJSON Feature (also serves as STAC Item for raster/pointcloud collections)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Feature {
    #[serde(rename = "type")]
    pub feature_type: String,
    pub id: String,
    pub geometry: serde_json::Value,
    pub properties: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<Link>>,
    /// STAC fields (for raster/pointcloud items)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<serde_json::Value>,
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
    #[serde(rename = "numberReturned", skip_serializing_if = "Option::is_none")]
    pub number_returned: Option<u64>,
    #[serde(rename = "timeStamp", skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// Request to create a feature or STAC item
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateFeatureRequest {
    #[serde(rename = "type")]
    pub feature_type: Option<String>,
    pub geometry: serde_json::Value,
    pub properties: serde_json::Value,
    /// STAC item assets (for raster/pointcloud collections)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<serde_json::Value>,
}

/// Request to update a feature or STAC item (PATCH - JSON Merge Patch)
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateFeatureRequest {
    pub geometry: Option<serde_json::Value>,
    pub properties: Option<serde_json::Value>,
    /// STAC item assets (for raster/pointcloud collections)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<serde_json::Value>,
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
    State((service, collection_service)): State<(Arc<FeatureService>, Arc<CollectionService>)>,
    path: CollectionItemsPath,
    Query(params): Query<FeatureQueryParams>,
) -> AppResult<(HeaderMap, Json<FeatureCollection>)> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = collection_service.check_alias_redirect(&collection_id).await? {
        return Err(AppError::NotFound(format!(
            "Collection moved to {}",
            new_name
        )));
    }

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
        number_returned: Some(features.len() as u64),
        features,
        links: Some(links),
        timestamp: Some(chrono::Utc::now().to_rfc3339()),
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, media_type::GEOJSON.parse().unwrap());
    headers.insert("Content-Crs", content_crs_header(response_crs).parse().unwrap());

    Ok((headers, Json(collection)))
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
    State((service, collection_service)): State<(Arc<FeatureService>, Arc<CollectionService>)>,
    path: FeaturePath,
    Query(params): Query<FeatureQueryParams>,
) -> AppResult<(HeaderMap, Json<Feature>)> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = collection_service.check_alias_redirect(&collection_id).await? {
        return Err(AppError::NotFound(format!(
            "Collection moved to {}",
            new_name
        )));
    }

    let feature_id = path.feature_id;
    let target_crs = parse_crs_param(params.crs.as_deref())?;

    let (feature, version, storage_srid) = service
        .get_feature(&user.username, &collection_id, feature_id, target_crs)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Feature {} not found in collection {}",
                feature_id, collection_id
            ))
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

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, media_type::GEOJSON.parse().unwrap());
    headers.insert("Content-Crs", content_crs_header(response_crs).parse().unwrap());
    headers.insert(header::ETAG, format!("\"{}\"", version).parse().unwrap());

    Ok((headers, Json(feature)))
}

fn get_feature_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get feature")
        .description("Returns a single feature by ID from a collection")
        .tag("Features")
        .response_with::<200, Json<Feature>, _>(|res| {
            res.description("Feature details")
        })
        .response_with::<404, (), _>(|res| res.description("Feature not found"))
}

pub async fn create_feature(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((service, collection_service)): State<(Arc<FeatureService>, Arc<CollectionService>)>,
    path: CollectionItemsPath,
    Json(request): Json<CreateFeatureRequest>,
) -> AppResult<(StatusCode, HeaderMap, Json<Feature>)> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = collection_service.check_alias_redirect(&collection_id).await? {
        return Err(AppError::NotFound(format!(
            "Collection moved to {}",
            new_name
        )));
    }

    // Extract datetime from properties if present
    let datetime = request
        .properties
        .get("datetime")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    // Try to create as STAC item first (for raster/pointcloud), fall back to vector feature
    let (feature, version) = if request.assets.is_some() {
        // Has assets, must be a STAC item
        service
            .create_item(
                &user.username,
                &collection_id,
                &request.geometry,
                &request.properties,
                datetime,
                request.assets.as_ref(),
            )
            .await?
    } else {
        // Try as vector feature first
        match service
            .create_feature(
                &user.username,
                &collection_id,
                &request.geometry,
                &request.properties,
            )
            .await
        {
            Ok(result) => result,
            Err(AppError::BadRequest(msg)) if msg.contains("vector") => {
                // Collection is not vector, try as STAC item without assets
                service
                    .create_item(
                        &user.username,
                        &collection_id,
                        &request.geometry,
                        &request.properties,
                        datetime,
                        None,
                    )
                    .await?
            }
            Err(e) => return Err(e),
        }
    };

    let base_url = &config.base_url;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, media_type::GEOJSON.parse().unwrap());
    headers.insert(
        header::LOCATION,
        format!(
            "{}/collections/{}/items/{}",
            base_url, collection_id, feature.id
        )
        .parse()
        .unwrap(),
    );
    headers.insert(header::ETAG, format!("\"{}\"", version).parse().unwrap());

    Ok((StatusCode::CREATED, headers, Json(feature)))
}

fn create_feature_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create feature")
        .description("Creates a new feature in a collection. Supports both vector features and STAC items.")
        .tag("Features")
        .response_with::<201, Json<Feature>, _>(|res| {
            res.description("Feature created successfully")
        })
        .response_with::<400, (), _>(|res| res.description("Invalid request"))
}

pub async fn update_feature(
    Extension(_config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((service, collection_service)): State<(Arc<FeatureService>, Arc<CollectionService>)>,
    path: FeaturePath,
    headers: HeaderMap,
    Json(request): Json<UpdateFeatureRequest>,
) -> AppResult<(HeaderMap, Json<Feature>)> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = collection_service.check_alias_redirect(&collection_id).await? {
        return Err(AppError::NotFound(format!(
            "Collection moved to {}",
            new_name
        )));
    }

    let feature_id = path.feature_id;
    // If-Match header is optional - when present, enables optimistic locking
    let expected_version: Option<i64> = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|etag| {
            etag.trim_matches('"')
                .parse()
                .map_err(|_| AppError::BadRequest("Invalid ETag format".to_string()))
        })
        .transpose()?;

    // Extract datetime from properties if present
    let datetime = request
        .properties
        .as_ref()
        .and_then(|p| p.get("datetime"))
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    // Try vector update first, fall back to item update
    let (feature, new_version) = match service
        .update_feature(
            &user.username,
            &collection_id,
            feature_id,
            expected_version,
            request.geometry.as_ref(),
            request.properties.as_ref(),
        )
        .await
    {
        Ok(result) => result,
        Err(AppError::NotFound(_)) if request.assets.is_some() || datetime.is_some() => {
            // May be a STAC item, try that
            service
                .update_item(
                    &user.username,
                    &collection_id,
                    feature_id,
                    expected_version,
                    request.geometry.as_ref(),
                    request.properties.as_ref(),
                    datetime,
                    request.assets.as_ref(),
                )
                .await?
        }
        Err(AppError::BadRequest(msg)) if msg.contains("vector") => {
            // Collection is not vector, try as STAC item
            service
                .update_item(
                    &user.username,
                    &collection_id,
                    feature_id,
                    expected_version,
                    request.geometry.as_ref(),
                    request.properties.as_ref(),
                    datetime,
                    request.assets.as_ref(),
                )
                .await?
        }
        Err(e) => return Err(e),
    };

    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::ETAG, format!("\"{}\"", new_version).parse().unwrap());

    Ok((response_headers, Json(feature)))
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
    Extension(_config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((service, collection_service)): State<(Arc<FeatureService>, Arc<CollectionService>)>,
    path: FeaturePath,
    headers: HeaderMap,
    Json(request): Json<CreateFeatureRequest>,
) -> AppResult<(HeaderMap, Json<Feature>)> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = collection_service.check_alias_redirect(&collection_id).await? {
        return Err(AppError::NotFound(format!(
            "Collection moved to {}",
            new_name
        )));
    }

    let feature_id = path.feature_id;
    // If-Match header is optional - when present, enables optimistic locking
    let expected_version: Option<i64> = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|etag| {
            etag.trim_matches('"')
                .parse()
                .map_err(|_| AppError::BadRequest("Invalid ETag format".to_string()))
        })
        .transpose()?;

    // Extract datetime from properties if present
    let datetime = request
        .properties
        .get("datetime")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    // Try vector replace first, fall back to item replace
    let (feature, new_version) = if request.assets.is_some() {
        // Has assets, must be a STAC item
        service
            .replace_item(
                &user.username,
                &collection_id,
                feature_id,
                expected_version,
                &request.geometry,
                &request.properties,
                datetime,
                request.assets.as_ref(),
            )
            .await?
    } else {
        match service
            .replace_feature(
                &user.username,
                &collection_id,
                feature_id,
                expected_version,
                &request.geometry,
                &request.properties,
            )
            .await
        {
            Ok(result) => result,
            Err(AppError::NotFound(_)) => {
                // May be a STAC item
                service
                    .replace_item(
                        &user.username,
                        &collection_id,
                        feature_id,
                        expected_version,
                        &request.geometry,
                        &request.properties,
                        datetime,
                        None,
                    )
                    .await?
            }
            Err(e) => return Err(e),
        }
    };

    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::ETAG, format!("\"{}\"", new_version).parse().unwrap());

    Ok((response_headers, Json(feature)))
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
    Extension(_config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((service, collection_service)): State<(Arc<FeatureService>, Arc<CollectionService>)>,
    path: FeaturePath,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = collection_service.check_alias_redirect(&collection_id).await? {
        return Err(AppError::NotFound(format!(
            "Collection moved to {}",
            new_name
        )));
    }

    let feature_id = path.feature_id;
    // If-Match header is optional - when present, enables optimistic locking
    let expected_version: Option<i64> = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|etag| {
            etag.trim_matches('"')
                .parse()
                .map_err(|_| AppError::BadRequest("Invalid ETag format".to_string()))
        })
        .transpose()?;

    // Try vector delete first, fall back to item delete
    match service
        .delete_feature(&user.username, &collection_id, feature_id, expected_version)
        .await
    {
        Ok(()) => {}
        Err(AppError::NotFound(_)) => {
            // May be a STAC item
            service
                .delete_item(&user.username, &collection_id, feature_id, expected_version)
                .await?;
        }
        Err(e) => return Err(e),
    }

    Ok(StatusCode::NO_CONTENT)
}

fn delete_feature_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Delete feature")
        .description("Deletes a feature from a collection. If-Match header is optional; when provided, enables optimistic locking.")
        .tag("Features")
        .response_with::<204, (), _>(|res| res.description("Feature deleted"))
        .response_with::<404, (), _>(|res| res.description("Feature not found"))
        .response_with::<412, (), _>(|res| res.description("Precondition failed (ETag mismatch)"))
}

pub fn routes(service: Arc<FeatureService>, collection_service: Arc<CollectionService>) -> ApiRouter {
    ApiRouter::new()
        .api_route(
            "/collections/{collection_id}/items",
            get_with(list_features, list_features_docs)
                .post_with(create_feature, create_feature_docs),
        )
        .api_route(
            "/collections/{collection_id}/items/{feature_id}",
            get_with(get_feature, get_feature_docs)
                .put_with(replace_feature, replace_feature_docs)
                .patch_with(update_feature, update_feature_docs)
                .delete_with(delete_feature, delete_feature_docs),
        )
        .with_state((service, collection_service))
}

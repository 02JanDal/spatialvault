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
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

use super::schemas::{
    CollectionResponse, CollectionSchema, CollectionsResponse, CreateCollectionRequest,
    ListCollectionsParams, UpdateCollectionRequest,
};
use crate::api::common::{crs, media_type, rel, Extent, Link};
use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::db::Collection;
use crate::error::{AppError, AppResult};
use crate::services::CollectionService;

/// Helper function to build a CollectionResponse from a Collection
/// This ensures consistent structure between list and get endpoints
/// by using the same link building logic.
/// 
/// Both endpoints now include all links (self, items, parent, schema, type-specific)
/// as per OGC API Features specification for consistency.
/// Both extent and storage_crs are computed from the database for all endpoints
/// to ensure complete and accurate collection metadata.
fn build_collection_response(
    collection: &Collection,
    base_url: &str,
    extent: Option<Extent>,
    storage_crs: i32,
    include_extended_links: bool,
) -> CollectionResponse {
    let id = &collection.canonical_name;
    
    // Base links that always appear
    let mut links = vec![
        Link::new(format!("{}/collections/{}", base_url, id), rel::SELF)
            .with_type(media_type::JSON),
        Link::new(format!("{}/collections/{}/items", base_url, id), rel::ITEMS)
            .with_type(media_type::GEOJSON),
    ];
    
    // Add extended links for complete collection metadata
    // Both list and detail endpoints now include these for consistency
    if include_extended_links {
        // Parent link back to collections list
        links.push(
            Link::new(format!("{}/collections", base_url), rel::PARENT)
                .with_type(media_type::JSON),
        );
        
        // Add type-specific links
        match collection.collection_type.as_str() {
            "vector" => {
                links.push(
                    Link::new(format!("{}/collections/{}/tiles", base_url, id), "tiles")
                        .with_type(media_type::JSON),
                );
            }
            "raster" => {
                links.push(
                    Link::new(
                        format!("{}/collections/{}/coverage", base_url, id),
                        "coverage",
                    )
                    .with_type(media_type::JSON),
                );
            }
            _ => {}
        }
        
        // Add schema link
        links.push(
            Link::new(
                format!("{}/collections/{}/schema", base_url, id),
                "describedby",
            )
            .with_type(media_type::JSON)
            .with_title("Schema for this collection"),
        );
    }
    
    CollectionResponse {
        id: id.clone(),
        title: collection.title.clone(),
        description: collection.description.clone(),
        links,
        extent,
        item_type: Some("feature".to_string()),
        crs: Some(vec![crs::WGS84.to_string(), crs::EPSG_3857.to_string()]),
        storage_crs: Some(crs::srid_to_uri(storage_crs)),
    }
}

pub async fn list_collections(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    Query(params): Query<ListCollectionsParams>,
) -> AppResult<Json<CollectionsResponse>> {
    let collections = service
        .list_collections(&user.username, params.limit, params.offset)
        .await?;

    let base_url = &config.base_url;

    // Compute extent for each collection
    let mut collection_responses = Vec::with_capacity(collections.len());
    for c in collections.iter() {
        let extent = service.compute_extent(&c.as_collection()).await?;
        collection_responses.push(build_collection_response(
            &c.as_collection(),
            base_url,
            extent,
            c.storage_crs,
            true,
        ));
    }

    let response = CollectionsResponse {
        collections: collection_responses,
        links: vec![
            Link::new(format!("{}/collections", base_url), rel::SELF).with_type(media_type::JSON),
        ],
        number_matched: None,
        number_returned: None,
    };

    Ok(Json(response))
}

fn list_collections_docs(op: TransformOperation) -> TransformOperation {
    op.summary("List collections")
        .description("Returns a list of all collections accessible to the authenticated user")
        .tag("Collections")
        .response_with::<200, Json<CollectionsResponse>, _>(|res| {
            res.description("List of collections")
        })
}

/// Path parameters for single collection endpoints
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}")]
pub struct CollectionPath {
    /// The collection identifier
    pub collection_id: String,
}

pub async fn get_collection(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    path: CollectionPath,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = service.check_alias_redirect(&collection_id).await? {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::LOCATION,
            format!("{}/collections/{}", config.base_url, new_name)
                .parse()
                .map_err(|_| AppError::Internal("Invalid redirect URL".to_string()))?,
        );
        return Ok((StatusCode::TEMPORARY_REDIRECT, headers).into_response());
    }

    let collection = service
        .get_collection(&user.username, &collection_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

    // Get computed extent
    let extent = service.compute_extent(&collection.as_collection()).await?;

    let base_url = &config.base_url;

    // Build the response using the common helper, with all links included
    let response = build_collection_response(&collection.as_collection(), base_url, extent, collection.storage_crs, true);

    // Create ETag from version
    let etag = format!("\"{}\"", collection.version);
    let mut headers = HeaderMap::new();
    let etag_value = etag
        .parse()
        .map_err(|_| AppError::Internal("Invalid ETag format".to_string()))?;
    headers.insert(header::ETAG, etag_value);

    Ok((headers, Json(response)).into_response())
}

fn get_collection_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get collection")
        .description("Returns the metadata for a specific collection")
        .tag("Collections")
        .response_with::<200, Json<CollectionResponse>, _>(|res| {
            res.description("Collection metadata")
        })
        .response_with::<404, (), _>(|res| res.description("Collection not found"))
}

pub async fn create_collection(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    Json(request): Json<CreateCollectionRequest>,
) -> AppResult<(StatusCode, HeaderMap, Json<CollectionResponse>)> {
    // Determine canonical name (prepend username if not already prefixed)
    let canonical_name = if request.id.starts_with(&format!("{}:", user.username)) {
        request.id.clone()
    } else {
        format!("{}:{}", user.username, request.id)
    };

    // Determine owner (default to current user)
    let owner = request.owner.unwrap_or_else(|| user.username.clone());

    // Validate owner (user can only create in their own namespace or groups they belong to)
    if owner != user.username && !user.groups.contains(&owner) {
        return Err(AppError::Forbidden(format!(
            "Cannot create collection owned by {}",
            owner
        )));
    }

    let collection = service
        .create_collection(
            &user.username,
            &canonical_name,
            &owner,
            &request.title,
            request.description.as_deref(),
            &request.collection_type,
            request.crs,
        )
        .await?;

    let base_url = &config.base_url;

    // Build response using the common helper to ensure consistency
    // Include extent and storage_crs based on the request CRS
    let response = build_collection_response(
        &collection,
        base_url,
        None,  // extent not computed for create response
        request.crs,  // storage_crs from request
        true,  // include all links for consistency
    );

    let mut headers = HeaderMap::new();
    let location_value = format!("{}/collections/{}", base_url, &collection.canonical_name)
        .parse()
        .map_err(|_| AppError::Internal("Invalid location URL".to_string()))?;
    headers.insert(header::LOCATION, location_value);
    let etag_value = format!("\"{}\"", collection.version)
        .parse()
        .map_err(|_| AppError::Internal("Invalid ETag format".to_string()))?;
    headers.insert(header::ETAG, etag_value);

    Ok((StatusCode::CREATED, headers, Json(response)))
}

fn create_collection_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create collection")
        .description("Creates a new collection owned by the authenticated user")
        .tag("Collections")
        .response_with::<201, Json<CollectionResponse>, _>(|res| {
            res.description("Collection created successfully")
        })
        .response_with::<400, (), _>(|res| res.description("Invalid request"))
        .response_with::<403, (), _>(|res| res.description("Permission denied"))
}

/// PATCH - Partial update using JSON Merge Patch (RFC 7386)
pub async fn patch_collection(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    path: CollectionPath,
    headers: HeaderMap,
    Json(request): Json<UpdateCollectionRequest>,
) -> AppResult<(HeaderMap, Json<CollectionResponse>)> {
    let collection_id = path.collection_id;
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

    let collection = service
        .update_collection(
            &user.username,
            &collection_id,
            expected_version,
            request.title.as_deref(),
            request.description.as_deref(),
            request.id.as_deref(),
        )
        .await?;

    let base_url = &config.base_url;

    // Fetch storage_crs from database
    let storage_crs = service.get_storage_crs(&collection).await?.unwrap_or(4326);

    // Build response using the common helper to ensure consistency
    let response = build_collection_response(
        &collection,
        base_url,
        None,  // extent not computed for patch response
        storage_crs,
        true,  // include all links for consistency
    );

    let mut response_headers = HeaderMap::new();
    let etag_value = format!("\"{}\"", collection.version)
        .parse()
        .map_err(|_| AppError::Internal("Invalid ETag format".to_string()))?;
    response_headers.insert(header::ETAG, etag_value);

    Ok((response_headers, Json(response)))
}

fn patch_collection_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Update collection (partial)")
        .description(
            "Partially updates a collection using JSON Merge Patch. If-Match header is optional; when provided, enables optimistic locking.",
        )
        .tag("Collections")
        .response_with::<200, Json<CollectionResponse>, _>(|res| {
            res.description("Collection updated successfully")
        })
        .response_with::<412, (), _>(|res| res.description("Precondition failed (ETag mismatch)"))
}

/// PUT - Full replacement of a collection
pub async fn update_collection(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    path: CollectionPath,
    headers: HeaderMap,
    Json(request): Json<CreateCollectionRequest>,
) -> AppResult<(HeaderMap, Json<CollectionResponse>)> {
    let collection_id = path.collection_id;
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

    // Validate that the ID in body matches the path (or is absent)
    // Per STAC spec, id in body should match path or server uses path id
    if request.id != collection_id
        && !request
            .id
            .ends_with(&format!(":{}", collection_id.split(':').last().unwrap_or(&collection_id)))
    {
        return Err(AppError::BadRequest(
            "Collection ID in body does not match path".to_string(),
        ));
    }

    let collection = service
        .replace_collection(
            &user.username,
            &collection_id,
            expected_version,
            &request.title,
            request.description.as_deref(),
        )
        .await?;

    let base_url = &config.base_url;

    // Fetch storage_crs from database
    let storage_crs = service.get_storage_crs(&collection).await?.unwrap_or(4326);

    // Build response using the common helper to ensure consistency
    let response = build_collection_response(
        &collection,
        base_url,
        None,  // extent not computed for put response
        storage_crs,
        true,  // include all links for consistency
    );

    let mut response_headers = HeaderMap::new();
    let etag_value = format!("\"{}\"", collection.version)
        .parse()
        .map_err(|_| AppError::Internal("Invalid ETag format".to_string()))?;
    response_headers.insert(header::ETAG, etag_value);

    Ok((response_headers, Json(response)))
}

fn update_collection_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Replace collection")
        .description("Fully replaces a collection. If-Match header is optional; when provided, enables optimistic locking.")
        .tag("Collections")
        .response_with::<200, Json<CollectionResponse>, _>(|res| {
            res.description("Collection replaced successfully")
        })
        .response_with::<412, (), _>(|res| res.description("Precondition failed (ETag mismatch)"))
}

pub async fn delete_collection(
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    path: CollectionPath,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let collection_id = path.collection_id;
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

    service
        .delete_collection(&user.username, &collection_id, expected_version)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

fn delete_collection_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Delete collection")
        .description("Deletes a collection. If-Match header is optional; when provided, enables optimistic locking.")
        .tag("Collections")
        .response_with::<204, (), _>(|res| res.description("Collection deleted"))
        .response_with::<412, (), _>(|res| res.description("Precondition failed (ETag mismatch)"))
}

/// Path parameters for collection schema endpoint
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/schema")]
pub struct CollectionSchemaPath {
    /// The collection identifier
    pub collection_id: String,
}

pub async fn get_collection_schema(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    path: CollectionSchemaPath,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = service.check_alias_redirect(&collection_id).await? {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::LOCATION,
            format!("{}/collections/{}/schema", config.base_url, new_name)
                .parse()
                .map_err(|_| AppError::Internal("Invalid redirect URL".to_string()))?,
        );
        return Ok((StatusCode::TEMPORARY_REDIRECT, headers).into_response());
    }

    let schema = service
        .get_collection_schema(&user.username, &collection_id)
        .await?;

    Ok(Json(schema).into_response())
}

fn get_collection_schema_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get collection schema")
        .description("Returns the JSON Schema describing features in this collection")
        .tag("Collections")
        .response_with::<200, Json<CollectionSchema>, _>(|res| {
            res.description("Collection schema")
        })
}

pub fn routes(service: Arc<CollectionService>) -> ApiRouter {
    ApiRouter::new()
        .api_route(
            "/collections",
            get_with(list_collections, list_collections_docs)
                .post_with(create_collection, create_collection_docs),
        )
        .api_route(
            "/collections/{collection_id}",
            get_with(get_collection, get_collection_docs)
                .put_with(update_collection, update_collection_docs)
                .patch_with(patch_collection, patch_collection_docs)
                .delete_with(delete_collection, delete_collection_docs),
        )
        .api_route(
            "/collections/{collection_id}/schema",
            get_with(get_collection_schema, get_collection_schema_docs),
        )
        .with_state(service)
}

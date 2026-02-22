use super::schemas::{
    CollectionResponse, CollectionSchema, CollectionsResponse, CreateCollectionRequest,
    ListCollectionsParams, UpdateCollectionRequest,
};
use crate::api::collections::CreateCollection;
use crate::api::common;
use crate::api::common::{Extent, Link, Location, crs, etag, media_type, rel};
use crate::api::processes::import_vector::ImportVectorInputs;
use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::db::Collection;
use crate::error::{AppError, AppResult, BadRequest, Forbidden};
use crate::services::{CollectionService, ProcessService};
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
use axum_extra::headers::{ETag, IfMatch};
use axum_extra::routing::TypedPath;
use axum_extra::{TypedHeader, headers};
use std::sync::Arc;
use std::time::SystemTime;
use uuid::Uuid;

// Type alias for the shared state tuple
type AppState = (Arc<S3Storage>, Arc<CollectionService>, Arc<ProcessService>);

/// Build the list of CRSes supported for retrieving features from a collection
/// Always includes WGS84, and adds storage CRS if it's different from WGS84
fn build_crs_list(storage_crs: Option<i32>) -> Vec<String> {
    let mut crs_list = vec![crs::WGS84.to_string()];

    // Add storage CRS if it's different from WGS84 (4326)
    if let Some(srid) = storage_crs {
        if srid != 4326 {
            crs_list.push(crs::srid_to_uri(srid));
        }
    }

    crs_list
}

/// Helper function to build a CollectionResponse from a Collection
/// This ensures consistent structure between list and get endpoints
/// by using the same link building logic.
///
/// The link structure differs between list and detail views:
/// - List: self, items, tiles/coverage (type-specific)
/// - Detail: self, items, parent, tiles/coverage, schema
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

    // Add type-specific links (always included for both list and detail)
    match collection.collection_type.as_str() {
        "vector" => {
            links.push(
                Link::new(format!("{}/collections/{}/tiles", base_url, id), "tiles")
                    .with_type(media_type::JSON),
            );
        }
        "raster" => {
            // Raster collections support both tiles and coverage endpoints
            links.push(
                Link::new(format!("{}/collections/{}/tiles", base_url, id), "tiles")
                    .with_type(media_type::JSON),
            );
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

    // Add extended links for detail view (parent and schema)
    if include_extended_links {
        // Parent link back to collections list
        links.push(
            Link::new(format!("{}/collections", base_url), rel::PARENT).with_type(media_type::JSON),
        );

        // Add schema link
        links.push(
            Link::new(
                format!("{}/collections/{}/schema", base_url, id),
                "describedby",
            )
            .with_type(media_type::JSON)
            .with_title("Schema for this collection"),
        );

        // Add queryables link
        links.push(
            Link::new(
                format!("{}/collections/{}/queryables", base_url, id),
                rel::QUERYABLES,
            )
            .with_type(media_type::SCHEMA_JSON)
            .with_title("Queryables for this collection"),
        );
    }

    CollectionResponse {
        id: id.clone(),
        title: collection.title.clone(),
        description: collection.description.clone(),
        links,
        extent,
        item_type: Some("feature".to_string()),
        crs: Some(build_crs_list(Some(storage_crs))),
        storage_crs: Some(crs::srid_to_uri(storage_crs)),
    }
}

pub async fn list_collections(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((_storage, service, _process_service)): State<AppState>,
    Query(params): Query<ListCollectionsParams>,
) -> AppResult<Json<CollectionsResponse>> {
    let (collections, total_count) = service
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
            false, // List view: don't include parent and schema links
        ));
    }

    let response = CollectionsResponse {
        number_returned: collection_responses.len() as u64,
        collections: collection_responses,
        links: vec![
            Link::new(format!("{}/collections", base_url), rel::SELF).with_type(media_type::JSON),
        ],
        number_matched: total_count as u64,
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
    State((_storage, service, _process_service)): State<AppState>,
    path: CollectionPath,
) -> Result<Response, AppError> {
    let collection = service
        .get_collection(&user.username, &path.collection_id)
        .await?;

    // Get computed extent
    let extent = service.compute_extent(&collection.as_collection()).await?;

    let base_url = &config.base_url;

    // Build the response using the common helper, with all links included
    let response = build_collection_response(
        &collection.as_collection(),
        base_url,
        extent,
        collection.storage_crs,
        true,
    );

    Ok((
        TypedHeader(etag::make(collection.version)),
        TypedHeader(headers::LastModified::from(SystemTime::from(
            collection.updated_at,
        ))),
        Json(response),
    )
        .into_response())
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
    State((storage, service, _process_service)): State<AppState>,
    request: CreateCollectionRequest,
) -> AppResult<(
    StatusCode,
    TypedHeader<ETag>,
    TypedHeader<Location>,
    Json<CollectionResponse>,
)> {
    let (metadata, file) = match request {
        CreateCollectionRequest::Json(data) => (data, None),
        CreateCollectionRequest::Multipart { metadata, file } => (metadata, Some(file)),
    };

    // Determine owner (default to current user)
    let owner = metadata.owner.unwrap_or_else(|| user.username.clone());

    // Determine canonical name (prepend owner if not already prefixed)
    let canonical_name = if metadata.id.starts_with(&format!("{}:", owner)) {
        metadata.id.clone()
    } else {
        format!("{}:{}", owner, metadata.id)
    };

    // Validate owner (user can only create in their own namespace or groups they belong to)
    if owner != user.username && !user.groups.contains(&owner) {
        return Err(Forbidden {
            message: format!("Cannot create collection owned by {}", owner),
        }
        .build());
    }

    // Generate collection UUID upfront so we can reference it in import job inputs
    let collection_id = Uuid::new_v4();

    // If a file is present: detect columns and upload to S3 before creating collection
    let mut columns = metadata.columns.clone();
    let mut import_job_inputs = None;

    if let Some(ref file) = file {
        // Auto-detect columns from the uploaded file if none were specified
        if columns.is_none() {
            let file_data = file.data.clone();
            let filename = file.filename.clone();
            let detected_columns = tokio::task::spawn_blocking(move || {
                use crate::processing::vector::VectorImporter;

                let temp_path = std::env::temp_dir().join(format!("detect_{}", filename));
                let result = (|| {
                    std::fs::write(&temp_path, &file_data).ok()?;
                    let importer = VectorImporter::open(&temp_path).ok()?;
                    let cols = importer.get_field_definitions().ok()?;
                    if cols.is_empty() { None } else { Some(cols) }
                })();
                std::fs::remove_file(&temp_path).ok();
                result
            })
            .await
            .ok()
            .flatten();

            if let Some(cols) = detected_columns {
                columns = Some(cols);
            }
        }

        // Upload to S3 before creating the collection (if upload fails, no DB changes)
        let key = format!("{}-{}", Uuid::new_v4(), file.filename);
        storage.put(&key, file.data.clone()).await?;

        import_job_inputs = Some(
            serde_json::to_value(ImportVectorInputs {
                collection_id: collection_id.to_string(),
                file_key: key,
            })
            .unwrap(),
        );
    }

    let collection = service
        .create_collection(
            collection_id,
            &user.username,
            &canonical_name,
            &owner,
            &metadata.title,
            metadata.description.as_deref(),
            &metadata.collection_type,
            metadata.crs,
            columns.as_deref(),
            import_job_inputs
                .as_ref()
                .map(|inputs| (user.username.as_str(), "import-vector", inputs)),
        )
        .await?;

    let base_url = &config.base_url;

    // Build response using the common helper to ensure consistency
    // Include extent and storage_crs based on the request CRS
    let response = build_collection_response(
        &collection,
        base_url,
        None,         // extent not computed for create response
        metadata.crs, // storage_crs from request
        true,         // include all links for consistency
    );

    let location_value = format!("{}/collections/{}", base_url, &collection.canonical_name);

    Ok((
        StatusCode::CREATED,
        TypedHeader(etag::make(collection.version)),
        TypedHeader(Location(location_value)),
        Json(response),
    ))
}

/// Create a collection with file import (multipart/form-data)
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
    State((_storage, service, _process_service)): State<AppState>,
    path: CollectionPath,
    if_match: Option<TypedHeader<IfMatch>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<UpdateCollectionRequest>,
) -> AppResult<(StatusCode, TypedHeader<ETag>, Json<CollectionResponse>)> {
    let collection_id = path.collection_id;

    let collection = service
        .update_collection(
            &user.username,
            &collection_id,
            &common::fix_header(headers, if_match),
            request.title.as_deref(),
            request.description.as_deref(),
            request.id.as_deref(),
            request.add_columns.as_deref(),
            request.remove_columns.as_deref(),
        )
        .await?;

    // Build response using the common helper to ensure consistency
    let response = build_collection_response(
        &collection,
        &config.base_url,
        service.compute_extent(&collection).await?,
        service.get_storage_crs(&collection).await?.unwrap_or(4326),
        true, // include all links for consistency
    );

    Ok((
        StatusCode::OK,
        TypedHeader(etag::make(collection.version)),
        Json(response),
    ))
}

fn patch_collection_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Update collection (partial)")
        .description(
            "Partially updates a collection using JSON Merge Patch. If-Match header is required to prevent lost updates.",
        )
        .tag("Collections")
        .response_with::<200, Json<CollectionResponse>, _>(|res| {
            res.description("Collection updated successfully")
        })
        .response_with::<412, (), _>(|res| res.description("Precondition failed (ETag mismatch or missing)"))
}

/// PUT - Full replacement of a collection
pub async fn update_collection(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((_storage, service, _process_service)): State<AppState>,
    path: CollectionPath,
    if_match: Option<TypedHeader<IfMatch>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<CreateCollection>,
) -> AppResult<(StatusCode, TypedHeader<ETag>, Json<CollectionResponse>)> {
    let collection_id = path.collection_id;

    // Validate that the ID in body matches the path (or is absent)
    // Per STAC spec, id in body should match path or server uses path id
    if request.id != collection_id
        && !request.id.ends_with(&format!(
            ":{}",
            collection_id.split(':').last().unwrap_or(&collection_id)
        ))
    {
        return Err(BadRequest {
            message: "Collection ID in body does not match path".to_string(),
        }
        .build());
    }

    let collection = service
        .replace_collection(
            &user.username,
            &collection_id,
            &common::fix_header(headers, if_match),
            &request.title,
            request.description.as_deref(),
            request.columns.as_deref(),
        )
        .await?;

    // Build response using the common helper to ensure consistency
    let response = build_collection_response(
        &collection,
        &config.base_url,
        service.compute_extent(&collection).await?,
        service.get_storage_crs(&collection).await?.unwrap_or(4326),
        true, // include all links for consistency
    );

    Ok((
        StatusCode::OK,
        TypedHeader(etag::make(collection.version)),
        Json(response),
    ))
}

fn update_collection_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Replace collection")
        .description(
            "Fully replaces a collection. If-Match header is required to prevent lost updates.",
        )
        .tag("Collections")
        .response_with::<200, Json<CollectionResponse>, _>(|res| {
            res.description("Collection replaced successfully")
        })
        .response_with::<412, (), _>(|res| {
            res.description("Precondition failed (ETag mismatch or missing)")
        })
}

pub async fn delete_collection(
    Extension(user): Extension<AuthenticatedUser>,
    State((_storage, service, _process_service)): State<AppState>,
    path: CollectionPath,
    if_match: Option<TypedHeader<IfMatch>>,
    headers: axum::http::HeaderMap,
) -> AppResult<StatusCode> {
    service
        .delete_collection(
            &user.username,
            &path.collection_id,
            &common::fix_header(headers, if_match),
        )
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
    Extension(user): Extension<AuthenticatedUser>,
    State((_storage, service, _process_service)): State<AppState>,
    path: CollectionSchemaPath,
) -> Result<Response, AppError> {
    let schema = service
        .get_collection_schema(&user.username, &path.collection_id)
        .await?;

    Ok(Json(schema).into_response())
}

fn get_collection_schema_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get collection schema")
        .description("Returns the JSON Schema describing features in this collection")
        .tag("Collections")
        .response_with::<200, Json<CollectionSchema>, _>(|res| res.description("Collection schema"))
}

/// Path parameters for collection queryables endpoint
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/queryables")]
pub struct CollectionQueryablesPath {
    /// The collection identifier
    pub collection_id: String,
}

pub async fn get_collection_queryables(
    Extension(user): Extension<AuthenticatedUser>,
    State((_storage, service, _process_service)): State<AppState>,
    path: CollectionQueryablesPath,
) -> Result<Response, AppError> {
    let queryables = service
        .get_collection_queryables(&user.username, &path.collection_id)
        .await?;

    use std::str::FromStr;
    Ok((
        TypedHeader(headers::ContentType::from_str(media_type::SCHEMA_JSON).unwrap()),
        Json(queryables),
    )
        .into_response())
}

fn get_collection_queryables_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get collection queryables")
        .description("Returns the queryable properties for this collection that can be used in filter expressions")
        .tag("Collections")
        .response_with::<200, Json<CollectionSchema>, _>(|res| {
            res.description("Collection queryables")
        })
}

pub fn routes(
    storage: Arc<S3Storage>,
    service: Arc<CollectionService>,
    process_service: Arc<ProcessService>,
) -> ApiRouter {
    ApiRouter::new()
        .api_route(
            "/collections",
            get_with(list_collections, list_collections_docs)
                .post_with(create_collection, create_collection_docs),
        )
        .api_route(
            CollectionPath::PATH,
            get_with(get_collection, get_collection_docs)
                .put_with(update_collection, update_collection_docs)
                .patch_with(patch_collection, patch_collection_docs)
                .delete_with(delete_collection, delete_collection_docs),
        )
        .api_route(
            CollectionSchemaPath::PATH,
            get_with(get_collection_schema, get_collection_schema_docs),
        )
        .api_route(
            CollectionQueryablesPath::PATH,
            get_with(get_collection_queryables, get_collection_queryables_docs),
        )
        .with_state((storage, service, process_service))
}

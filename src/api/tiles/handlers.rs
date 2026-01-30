use aide::{
    axum::{
        routing::get_with,
        ApiRouter,
    },
    transform::TransformOperation,
};
use axum::{
    body::Body,
    extract::{Extension, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::raster::RasterFormat;
use super::vector::{tile_matrix_sets, validate_tile_coords};
use crate::api::common::{media_type, rel, Link};
use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::services::{CollectionService, TileService};

/// Query parameters for tile requests
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TileQueryParams {
    /// Output format: png, jpeg, webp
    #[serde(rename = "f")]
    pub format: Option<String>,
}

/// Negotiate raster tile format from Accept header and query parameter
fn negotiate_raster_format(
    headers: &HeaderMap,
    query_format: Option<&str>,
) -> RasterFormat {
    // Query parameter takes precedence
    if let Some(fmt) = query_format {
        if let Some(format) = RasterFormat::from_extension(fmt) {
            return format;
        }
    }

    // Check Accept header
    if let Some(accept) = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()) {
        // Parse Accept header (simplified - doesn't handle quality values)
        for media_type in accept.split(',').map(|s| s.trim().split(';').next().unwrap_or("")) {
            match media_type {
                "image/jpeg" | "image/jpg" => return RasterFormat::Jpeg,
                "image/webp" => return RasterFormat::WebP,
                "image/png" => return RasterFormat::Png,
                "*/*" | "image/*" => return RasterFormat::Png, // Default for wildcards
                _ => continue,
            }
        }
    }

    // Default to PNG
    RasterFormat::Png
}

/// TileMatrixSet reference
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TileMatrixSetRef {
    pub id: String,
    pub title: Option<String>,
    pub uri: String,
}

/// TileMatrixSet list response
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TileMatrixSetListResponse {
    pub tile_matrix_sets: Vec<TileMatrixSetRef>,
}

/// Tileset metadata
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TilesetMetadata {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub data_type: String,
    pub crs: String,
    pub tile_matrix_set_id: String,
    pub links: Vec<Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tile_matrix_set_limits: Option<Vec<TileMatrixSetLimit>>,
}

/// Tile matrix set limit
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TileMatrixSetLimit {
    pub tile_matrix: String,
    pub min_tile_row: u32,
    pub max_tile_row: u32,
    pub min_tile_col: u32,
    pub max_tile_col: u32,
}

/// List available tile matrix sets
pub async fn list_tile_matrix_sets() -> Json<TileMatrixSetListResponse> {
    let tile_matrix_sets = vec![
        TileMatrixSetRef {
            id: tile_matrix_sets::WEB_MERCATOR_QUAD.to_string(),
            title: Some("Google Maps Compatible for the World".to_string()),
            uri: "http://www.opengis.net/def/tilematrixset/OGC/1.0/WebMercatorQuad".to_string(),
        },
        TileMatrixSetRef {
            id: tile_matrix_sets::WORLD_CRS84_QUAD.to_string(),
            title: Some("CRS84 for the World".to_string()),
            uri: "http://www.opengis.net/def/tilematrixset/OGC/1.0/WorldCRS84Quad".to_string(),
        },
    ];

    Json(TileMatrixSetListResponse { tile_matrix_sets })
}

fn list_tile_matrix_sets_docs(op: TransformOperation) -> TransformOperation {
    op.summary("List tile matrix sets")
        .description("Returns the list of supported tile matrix sets")
        .tag("Tiles")
        .response_with::<200, Json<TileMatrixSetListResponse>, _>(|res| {
            res.description("List of tile matrix sets")
        })
}

/// Path parameters for collection tiles endpoint
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/tiles")]
pub struct CollectionTilesPath {
    /// The collection identifier
    pub collection_id: String,
}

/// Get tileset metadata for a collection
pub async fn get_tileset(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((service, collection_service)): State<(Arc<TileService>, Arc<CollectionService>)>,
    path: CollectionTilesPath,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = collection_service.check_alias_redirect(&collection_id).await? {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::LOCATION,
            format!("{}/collections/{}/tiles", config.base_url, new_name)
                .parse()
                .map_err(|_| AppError::Internal("Invalid redirect URL".to_string()))?,
        );
        return Ok((StatusCode::TEMPORARY_REDIRECT, headers).into_response());
    }

    let collection = service
        .get_collection(&user.username, &collection_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

    let base_url = &config.base_url;

    // Determine tile type and content type based on collection type
    let data_type = match collection.collection_type.as_str() {
        "vector" => "vector",
        "raster" => "map",
        _ => "vector",
    };

    let mut links = vec![
        Link::new(
            format!("{}/collections/{}/tiles", base_url, collection_id),
            rel::SELF,
        )
        .with_type(media_type::JSON),
        Link::new(
            format!("{}/collections/{}", base_url, collection_id),
            rel::COLLECTION,
        )
        .with_type(media_type::JSON),
    ];

    // Add tile URL templates based on collection type
    if collection.collection_type == "vector" {
        links.push(
            Link::new(
                format!(
                    "{}/collections/{}/tiles/WebMercatorQuad/{{tileMatrix}}/{{tileRow}}/{{tileCol}}",
                    base_url, collection_id
                ),
                "item",
            )
            .with_type(media_type::MVT)
            .with_title("Vector tile (MVT)"),
        );
    } else if collection.collection_type == "raster" {
        // Add links for each supported format
        links.push(
            Link::new(
                format!(
                    "{}/collections/{}/tiles/WebMercatorQuad/{{tileMatrix}}/{{tileRow}}/{{tileCol}}?f=png",
                    base_url, collection_id
                ),
                "item",
            )
            .with_type("image/png")
            .with_title("Raster tile (PNG)"),
        );
        links.push(
            Link::new(
                format!(
                    "{}/collections/{}/tiles/WebMercatorQuad/{{tileMatrix}}/{{tileRow}}/{{tileCol}}?f=jpeg",
                    base_url, collection_id
                ),
                "item",
            )
            .with_type("image/jpeg")
            .with_title("Raster tile (JPEG)"),
        );
    }

    // For raster collections, add links to COG assets for direct access
    if collection.collection_type == "raster" {
        let assets = service.get_raster_assets(&collection_id).await?;
        for (item_id, href) in assets.iter().take(5) {
            // Limit to first 5
            links.push(
                Link::new(href, "enclosure")
                    .with_type("image/tiff; application=geotiff; profile=cloud-optimized")
                    .with_title(format!("COG asset for item {}", item_id)),
            );
        }
    }

    let tileset = TilesetMetadata {
        title: collection.title.clone(),
        description: collection.description.clone(),
        data_type: data_type.to_string(),
        crs: "http://www.opengis.net/def/crs/EPSG/0/3857".to_string(),
        tile_matrix_set_id: tile_matrix_sets::WEB_MERCATOR_QUAD.to_string(),
        links,
        tile_matrix_set_limits: None,
    };

    Ok(Json(tileset).into_response())
}

fn get_tileset_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get tileset metadata")
        .description("Returns tileset metadata for a collection, including tile URL templates")
        .tag("Tiles")
        .response_with::<200, Json<TilesetMetadata>, _>(|res| {
            res.description("Tileset metadata")
        })
        .response_with::<404, (), _>(|res| res.description("Collection not found"))
}

/// Path parameters for single tile endpoint
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/tiles/{tile_matrix_set_id}/{z}/{y}/{x}")]
pub struct TilePath {
    /// The collection identifier
    pub collection_id: String,
    /// The tile matrix set identifier (e.g., WebMercatorQuad)
    pub tile_matrix_set_id: String,
    /// Zoom level
    pub z: u32,
    /// Row (y) coordinate
    pub y: u32,
    /// Column (x) coordinate
    pub x: u32,
}

/// Get a single tile
pub async fn get_tile(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State((service, collection_service)): State<(Arc<TileService>, Arc<CollectionService>)>,
    path: TilePath,
    Query(params): Query<TileQueryParams>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = collection_service.check_alias_redirect(&collection_id).await? {
        let mut redirect_headers = HeaderMap::new();
        redirect_headers.insert(
            header::LOCATION,
            format!("{}/collections/{}/tiles/{}/{}/{}/{}", config.base_url, new_name, path.tile_matrix_set_id, path.z, path.y, path.x)
                .parse()
                .map_err(|_| AppError::Internal("Invalid redirect URL".to_string()))?,
        );
        return Ok((StatusCode::TEMPORARY_REDIRECT, redirect_headers).into_response());
    }

    let tile_matrix_set_id = path.tile_matrix_set_id;
    let z = path.z;
    let y = path.y;
    let x = path.x;
    // Validate tile matrix set
    if tile_matrix_set_id != tile_matrix_sets::WEB_MERCATOR_QUAD {
        return Err(AppError::NotFound(format!(
            "TileMatrixSet not supported: {}",
            tile_matrix_set_id
        )));
    }

    // Validate coordinates
    validate_tile_coords(z, x, y, 22)?;

    // Get collection to determine type
    let collection = service
        .get_collection(&user.username, &collection_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

    match collection.collection_type.as_str() {
        "vector" => {
            // Get MVT tile data
            let tile_data = service
                .get_vector_tile(&user.username, &collection_id, z, x, y)
                .await?;

            let mut response_headers = HeaderMap::new();
            response_headers.insert(header::CONTENT_TYPE, media_type::MVT.parse().unwrap());
            response_headers.insert(
                header::CACHE_CONTROL,
                "public, max-age=3600".parse().unwrap(),
            );

            Ok((StatusCode::OK, response_headers, Body::from(tile_data)).into_response())
        }
        "raster" => {
            // Negotiate format from Accept header and query parameter
            let format = negotiate_raster_format(&headers, params.format.as_deref());

            // Get raster tile in requested format
            let tile_data = service
                .get_raster_tile(&user.username, &collection_id, z, x, y, format)
                .await?;

            let mut response_headers = HeaderMap::new();
            response_headers.insert(
                header::CONTENT_TYPE,
                format.content_type().parse().unwrap(),
            );
            response_headers.insert(
                header::CACHE_CONTROL,
                "public, max-age=3600".parse().unwrap(),
            );
            // Add Vary header for proper caching with content negotiation
            response_headers.insert(header::VARY, "Accept".parse().unwrap());

            Ok((StatusCode::OK, response_headers, Body::from(tile_data)).into_response())
        }
        "pointcloud" => {
            Err(AppError::BadRequest(
                "Tiles not available for point cloud collections. Use STAC items endpoint to access COPC files.".to_string(),
            ))
        }
        _ => Err(AppError::BadRequest(format!(
            "Unknown collection type: {}",
            collection.collection_type
        ))),
    }
}

fn get_tile_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get tile")
        .description("Returns a single tile as MVT (vector) or PNG/JPEG/WebP (raster)")
        .tag("Tiles")
        .response_with::<200, (), _>(|res| {
            res.description("Tile data (application/vnd.mapbox-vector-tile or image/*)")
        })
        .response_with::<404, (), _>(|res| res.description("Collection or tile not found"))
}

pub fn routes(service: Arc<TileService>, collection_service: Arc<CollectionService>) -> ApiRouter {
    ApiRouter::new()
        .api_route("/tileMatrixSets", get_with(list_tile_matrix_sets, list_tile_matrix_sets_docs))
        .api_route("/collections/{collection_id}/tiles", get_with(get_tileset, get_tileset_docs))
        .api_route(
            "/collections/{collection_id}/tiles/{tile_matrix_set_id}/{z}/{y}/{x}",
            get_with(get_tile, get_tile_docs),
        )
        .with_state((service, collection_service))
}

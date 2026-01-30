//! OGC API Tiles Core conformance tests
//!
//! Implements abstract test requirements from:
//! http://www.opengis.net/spec/ogcapi-tiles-1/1.0/conf/core

use crate::common::{TestApp, test_collection_request, test_feature_request};
use axum::http::StatusCode;

/// Test TileMatrixSets endpoint
#[tokio::test]
async fn test_tile_matrix_sets() {
    let app = TestApp::new().await;

    let response = app.get("/tileMatrixSets").await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    // Should have tileMatrixSets array
    let tms = body["tileMatrixSets"]
        .as_array()
        .expect("tileMatrixSets must be an array");

    // Should include WebMercatorQuad
    let has_web_mercator = tms.iter().any(|t| {
        t["id"]
            .as_str()
            .map(|s| s == "WebMercatorQuad")
            .unwrap_or(false)
    });

    assert!(has_web_mercator, "Should support WebMercatorQuad");
}

/// Test tileset metadata for a collection
#[tokio::test]
async fn test_collection_tileset_metadata() {
    let app = TestApp::new().await;

    // Create a vector collection
    let collection = test_collection_request("tiles-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get tileset metadata
    let response = app
        .get(&format!("/collections/{}/tiles", collection_id))
        .await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    // Should have required properties
    assert!(body["title"].is_string(), "Tileset should have title");
    assert!(body["dataType"].is_string(), "Tileset should have dataType");
    assert!(body["crs"].is_string(), "Tileset should have crs");
    assert!(
        body["tileMatrixSetId"].is_string(),
        "Tileset should have tileMatrixSetId"
    );
    assert!(body["links"].is_array(), "Tileset should have links");
}

/// Test tile retrieval at various zoom levels
#[tokio::test]
async fn test_tile_retrieval() {
    let app = TestApp::new().await;

    // Create a vector collection with a feature
    let collection = test_collection_request("tile-retrieval-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Add a feature so there's data
    let feature = test_feature_request();
    let feature_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    feature_response.assert_status(StatusCode::CREATED);

    // Test tile at zoom level 0 (single world tile)
    let response = app
        .get(&format!(
            "/collections/{}/tiles/WebMercatorQuad/0/0/0",
            collection_id
        ))
        .await;

    response.assert_success();

    // Check content type (MVT for vector tiles)
    let content_type = response.header("content-type").unwrap_or_default();
    assert!(
        content_type.contains("mapbox-vector-tile") || content_type.contains("image/"),
        "Should return MVT or image tile, got: {}",
        content_type
    );
}

/// Test tile with valid coordinates
#[tokio::test]
async fn test_tile_valid_coordinates() {
    let app = TestApp::new().await;

    // Create a vector collection
    let collection = test_collection_request("tile-coords-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Test valid tile coordinates at zoom level 2
    // At zoom 2, valid x and y are 0-3
    let test_cases = vec![(2, 0, 0), (2, 1, 1), (2, 3, 3)];

    for (z, y, x) in test_cases {
        let response = app
            .get(&format!(
                "/collections/{}/tiles/WebMercatorQuad/{}/{}/{}",
                collection_id, z, y, x
            ))
            .await;

        assert!(
            response.status.is_success(),
            "Tile {}/{}/{} should be valid",
            z,
            y,
            x
        );
    }
}

/// Test tile with invalid coordinates returns 400 or 404
#[tokio::test]
async fn test_tile_invalid_coordinates() {
    let app = TestApp::new().await;

    // Create a vector collection
    let collection = test_collection_request("tile-invalid-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Test invalid tile coordinates
    // At zoom 2, max x and y is 3, so 4 is invalid
    let response = app
        .get(&format!(
            "/collections/{}/tiles/WebMercatorQuad/2/4/4",
            collection_id
        ))
        .await;

    // Should return 400 Bad Request or 404 Not Found for out-of-bounds coordinates
    // Both are valid per OGC API Tiles spec - 404 indicates the tile doesn't exist
    assert!(
        response.status == StatusCode::BAD_REQUEST || response.status == StatusCode::NOT_FOUND,
        "Should return 400 or 404 for invalid coordinates, got: {}",
        response.status
    );
}

/// Test unsupported TileMatrixSet returns 404
#[tokio::test]
async fn test_unsupported_tile_matrix_set() {
    let app = TestApp::new().await;

    // Create a vector collection
    let collection = test_collection_request("tile-tms-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Request with unsupported TileMatrixSet
    let response = app
        .get(&format!(
            "/collections/{}/tiles/UnsupportedTMS/0/0/0",
            collection_id
        ))
        .await;

    // Should return 404 Not Found
    response.assert_status(StatusCode::NOT_FOUND);
}

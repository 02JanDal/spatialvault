//! OGC API Features CRS conformance tests
//!
//! Implements abstract test requirements from:
//! http://www.opengis.net/spec/ogcapi-features-2/1.0/conf/crs

use axum::http::StatusCode;
use crate::common::{test_collection_request, test_feature_request, TestApp};

/// Test Content-Crs header in response
#[tokio::test]
async fn test_content_crs_header() {
    let app = TestApp::new().await;

    // Create a collection and feature first
    let collection = test_collection_request("crs-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create a feature
    let feature = test_feature_request();
    let feature_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    feature_response.assert_status(StatusCode::CREATED);

    // Get features
    let response = app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    response.assert_success();

    // Must have Content-Crs header
    let content_crs = response.header("Content-Crs");
    assert!(
        content_crs.is_some(),
        "Response must include Content-Crs header"
    );

    let crs_value = content_crs.unwrap();
    assert!(crs_value.starts_with('<'), "Content-Crs must start with <");
    assert!(crs_value.ends_with('>'), "Content-Crs must end with >");
}

/// Test crs parameter for response CRS transformation
#[tokio::test]
async fn test_crs_parameter() {
    let app = TestApp::new().await;

    // Create a collection and feature
    let collection = test_collection_request("crs-param-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create a feature
    let feature = test_feature_request();
    let feature_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    feature_response.assert_status(StatusCode::CREATED);

    // Request with Web Mercator CRS
    let response = app
        .get(&format!(
            "/collections/{}/items?crs=http://www.opengis.net/def/crs/EPSG/0/3857",
            collection_id
        ))
        .await;
    response.assert_success();

    // Content-Crs should match requested CRS
    let content_crs = response.header("Content-Crs").expect("Missing Content-Crs header");
    assert!(
        content_crs.contains("3857"),
        "Content-Crs should indicate EPSG:3857, got: {}",
        content_crs
    );
}

/// Test bbox-crs parameter
#[tokio::test]
async fn test_bbox_crs_parameter() {
    let app = TestApp::new().await;

    // Create a collection and feature
    let collection = test_collection_request("bbox-crs-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create a feature at origin
    let feature = test_feature_request();
    let feature_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    feature_response.assert_status(StatusCode::CREATED);

    // Request with bbox in Web Mercator (covers the world)
    let bbox = "-20037508,-20037508,20037508,20037508";
    let bbox_crs = "http://www.opengis.net/def/crs/EPSG/0/3857";

    let response = app
        .get(&format!(
            "/collections/{}/items?bbox={}&bbox-crs={}",
            collection_id, bbox, bbox_crs
        ))
        .await;

    // Should succeed
    response.assert_success();
}

/// Test storageCrs in collection metadata
#[tokio::test]
async fn test_storage_crs_in_collection() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("storage-crs-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get collection metadata
    let response = app.get(&format!("/collections/{}", collection_id)).await;
    response.assert_success();

    let body: serde_json::Value = response.json();

    // Should have crs list
    if let Some(crs) = body.get("crs") {
        assert!(crs.is_array(), "crs should be an array");
    }

    // storageCrs is optional but if present should be a string
    if let Some(storage_crs) = body.get("storageCrs") {
        assert!(storage_crs.is_string(), "storageCrs should be a string");
    }
}

/// Test that invalid CRS returns appropriate error
#[tokio::test]
async fn test_invalid_crs_returns_error() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("invalid-crs-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Request with invalid CRS
    let response = app
        .get(&format!(
            "/collections/{}/items?crs=invalid-crs",
            collection_id
        ))
        .await;

    // Should return 400 Bad Request
    response.assert_status(StatusCode::BAD_REQUEST);
}

//! OGC API Coverages Core conformance tests
//!
//! Implements abstract test requirements from:
//! http://www.opengis.net/spec/ogcapi-coverages-1/0.0/conf/core

use axum::http::StatusCode;
use crate::common::{test_collection_request, TestApp};

/// Test coverage description endpoint
#[tokio::test]
async fn test_coverage_description() {
    let app = TestApp::new().await;

    // Create a raster collection
    let collection = test_collection_request("coverage-test", "raster");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get coverage description
    let response = app
        .get(&format!("/collections/{}/coverage", collection_id))
        .await;

    // Raster collections should support coverage endpoint
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    assert!(body["id"].is_string(), "Coverage should have id");
    assert!(body["title"].is_string(), "Coverage should have title");
    assert!(body["links"].is_array(), "Coverage should have links");
}

/// Test domain set retrieval
#[tokio::test]
async fn test_domain_set_retrieval() {
    let app = TestApp::new().await;

    // Create a raster collection
    let collection = test_collection_request("domainset-test", "raster");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get domain set
    let response = app
        .get(&format!("/collections/{}/coverage/domainset", collection_id))
        .await;

    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    assert!(body["type"].is_string(), "DomainSet should have type");
    assert!(
        body["generalGrid"].is_object(),
        "DomainSet should have generalGrid"
    );
}

/// Test range type retrieval
#[tokio::test]
async fn test_range_type_retrieval() {
    let app = TestApp::new().await;

    // Create a raster collection
    let collection = test_collection_request("rangetype-test", "raster");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get range type
    let response = app
        .get(&format!("/collections/{}/coverage/rangetype", collection_id))
        .await;

    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    assert!(body["type"].is_string(), "RangeType should have type");
    assert!(body["field"].is_array(), "RangeType should have field array");
}

/// Test coverage endpoint returns 400 for non-raster collections
#[tokio::test]
async fn test_coverage_rejects_vector_collection() {
    let app = TestApp::new().await;

    // Create a vector collection
    let collection = test_collection_request("vector-coverage-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get coverage description - should fail for vector collection
    let response = app
        .get(&format!("/collections/{}/coverage", collection_id))
        .await;

    // Should return 400 Bad Request for non-raster collection
    response.assert_status(StatusCode::BAD_REQUEST);
}

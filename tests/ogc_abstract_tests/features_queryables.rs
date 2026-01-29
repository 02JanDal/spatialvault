//! OGC API Features - Part 3: Filtering - Queryables conformance tests
//!
//! Implements abstract test requirements from:
//! http://www.opengis.net/spec/ogcapi-features-3/1.0/conf/queryables
//!
//! Tests use TestApp with testcontainers for the database and mock authentication.

use axum::http::StatusCode;
use crate::common::{test_collection_request, test_feature_request, TestApp};

/// Test that queryables endpoint exists and returns valid JSON Schema
#[tokio::test]
async fn queryables_endpoint_returns_json_schema() {
    let app = TestApp::new().await;

    // Create a test collection
    let collection = test_collection_request("queryables-test-vector", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Test queryables endpoint
    let response = app.get(&format!("/collections/{}/queryables", collection_id)).await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    // Must have JSON Schema required fields
    assert!(body["$schema"].is_string(), "Must have $schema field");
    assert!(body["$id"].is_string(), "Must have $id field");
    assert!(body["type"].is_string(), "Must have type field");
    assert_eq!(body["type"].as_str(), Some("object"), "Type must be 'object'");
    
    // Must have properties
    assert!(body["properties"].is_object(), "Must have properties object");
    
    // Properties should include geometry
    let properties = body["properties"].as_object().expect("properties must be an object");
    assert!(properties.contains_key("geometry"), "Should have geometry property");
}

/// Test that queryables includes appropriate properties for vector collections
#[tokio::test]
async fn queryables_vector_collection_properties() {
    let app = TestApp::new().await;

    // Create a test vector collection
    let collection = test_collection_request("queryables-vector-props", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Add a feature to ensure table is created with properties
    let feature = test_feature_request();
    let feature_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    feature_response.assert_success();

    // Get queryables
    let response = app.get(&format!("/collections/{}/queryables", collection_id)).await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let properties = body["properties"].as_object().expect("properties must be an object");

    // Should have geometry
    assert!(properties.contains_key("geometry"), "Should have geometry property");
    
    // Should NOT have system fields like id, version, created_at, updated_at
    assert!(!properties.contains_key("id"), "Should not have id in queryables");
    assert!(!properties.contains_key("version"), "Should not have version in queryables");
    assert!(!properties.contains_key("created_at"), "Should not have created_at in queryables");
    assert!(!properties.contains_key("updated_at"), "Should not have updated_at in queryables");
}

/// Test that queryables includes appropriate properties for raster/pointcloud collections
#[tokio::test]
async fn queryables_raster_collection_properties() {
    let app = TestApp::new().await;

    // Create a test raster collection
    let collection = test_collection_request("queryables-raster-props", "raster");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get queryables
    let response = app.get(&format!("/collections/{}/queryables", collection_id)).await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let properties = body["properties"].as_object().expect("properties must be an object");

    // Raster/pointcloud items should have geometry, datetime, and properties
    assert!(properties.contains_key("geometry"), "Should have geometry property");
    assert!(properties.contains_key("datetime"), "Should have datetime property");
    assert!(properties.contains_key("properties"), "Should have properties property");
}

/// Test that collection response includes link to queryables
#[tokio::test]
async fn collection_links_to_queryables() {
    let app = TestApp::new().await;

    // Create a test collection
    let collection = test_collection_request("queryables-link-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get the collection
    let response = app.get(&format!("/collections/{}", collection_id)).await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let links = body["links"].as_array().expect("links must be an array");

    // Should have a link to queryables with the correct rel
    let has_queryables_link = links.iter().any(|link| {
        link.get("rel")
            .and_then(|r| r.as_str())
            .map(|r| r == "http://www.opengis.net/def/rel/ogc/1.0/queryables")
            .unwrap_or(false)
    });

    assert!(has_queryables_link, "Collection should have link to queryables with correct rel type");

    // Verify the link points to the correct endpoint
    let queryables_link = links.iter().find(|link| {
        link.get("rel")
            .and_then(|r| r.as_str())
            .map(|r| r == "http://www.opengis.net/def/rel/ogc/1.0/queryables")
            .unwrap_or(false)
    });

    assert!(queryables_link.is_some(), "Should have queryables link");
    
    let href = queryables_link
        .and_then(|l| l.get("href"))
        .and_then(|h| h.as_str())
        .expect("Link should have href");
    
    assert!(href.contains(&format!("/collections/{}/queryables", collection_id)), 
            "Link should point to queryables endpoint");
}

/// Test that conformance declaration includes queryables
#[tokio::test]
async fn conformance_includes_queryables() {
    let app = TestApp::new().await;

    let response = app.get("/conformance").await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let conforms_to = body["conformsTo"]
        .as_array()
        .expect("conformsTo must be an array");

    // Must include Queryables conformance class
    let has_queryables = conforms_to.iter().any(|c| {
        c.as_str()
            .map(|s| s.contains("ogcapi-features-3") && s.contains("queryables"))
            .unwrap_or(false)
    });

    assert!(has_queryables, "Must declare Queryables conformance");
}

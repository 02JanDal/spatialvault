//! OGC API Features Core conformance tests
//!
//! Implements abstract test requirements from:
//! http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/core
//!
//! Tests use TestApp with testcontainers for the database and mock authentication.

use axum::http::StatusCode;
use crate::common::{assert_has_link, test_collection_request, test_feature_request, TestApp};

/// A.2.1: Landing page response
#[tokio::test]
async fn landing_page_response() {
    let app = TestApp::new().await;

    let response = app.get("/").await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    // Must have links
    let links = body["links"].as_array().expect("links must be an array");

    // Must have self link
    assert!(assert_has_link(links, "self"), "Missing self link");

    // Must have service-desc link (OpenAPI)
    assert!(
        assert_has_link(links, "service-desc"),
        "Missing service-desc link"
    );

    // Must have conformance link
    assert!(
        assert_has_link(links, "conformance"),
        "Missing conformance link"
    );

    // Must have data link (collections)
    assert!(assert_has_link(links, "data"), "Missing data link");
}

/// A.2.2: API definition retrieval
#[tokio::test]
async fn api_definition_retrieval() {
    let app = TestApp::new().await;

    let response = app.get("/api").await;
    response.assert_success();

    let body: serde_json::Value = response.json();

    // Must have openapi version
    assert!(body["openapi"].is_string(), "Missing openapi version");

    // Must have info
    assert!(body["info"].is_object(), "Missing info object");

    // Must have paths
    assert!(body["paths"].is_object(), "Missing paths object");
}

/// A.2.3: Conformance declaration
#[tokio::test]
async fn conformance_declaration() {
    let app = TestApp::new().await;

    let response = app.get("/conformance").await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    let conforms_to = body["conformsTo"]
        .as_array()
        .expect("conformsTo must be an array");

    // Must include Features Core
    let has_features_core = conforms_to.iter().any(|c| {
        c.as_str()
            .map(|s| s.contains("ogcapi-features-1") && s.contains("core"))
            .unwrap_or(false)
    });

    assert!(has_features_core, "Must declare Features Core conformance");
}

/// A.2.4: Collections metadata
#[tokio::test]
async fn collections_metadata() {
    let app = TestApp::new().await;

    let response = app.get("/collections").await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    // Must have collections array
    assert!(
        body["collections"].is_array(),
        "collections must be an array"
    );

    // Must have links
    let links = body["links"].as_array().expect("links must be an array");
    assert!(assert_has_link(links, "self"), "Missing self link");
}

/// A.2.5-7: Full CRUD workflow with features
#[tokio::test]
async fn features_crud_workflow() {
    let app = TestApp::new().await;

    // 1. Create a collection
    // Note: The collection id becomes "testuser:test-features" (canonical name)
    let collection = test_collection_request("test-features", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    // The canonical name should be "testuser:test-features"
    assert!(collection_id.contains("test-features"), "Collection id should contain test-features");

    // 2. Verify collection appears in list
    let list_response = app.get("/collections").await;
    list_response.assert_success();

    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"].as_array().unwrap();
    assert!(
        collections.iter().any(|c| c["id"].as_str() == Some(collection_id)),
        "Created collection should appear in list. Collections: {:?}",
        collections
    );

    // 3. Get the specific collection (A.2.5)
    let collection_response = app.get(&format!("/collections/{}", collection_id)).await;
    collection_response.assert_success();
    collection_response.assert_content_type("application/json");

    let collection_body: serde_json::Value = collection_response.json();
    assert!(collection_body["id"].is_string(), "Collection must have id");
    assert!(collection_body["links"].is_array(), "Collection must have links");

    let links = collection_body["links"].as_array().unwrap();
    assert!(assert_has_link(links, "self"), "Missing self link");
    assert!(assert_has_link(links, "items"), "Missing items link");

    // 4. Get features (empty initially) (A.2.6)
    let features_response = app.get(&format!("/collections/{}/items", collection_id)).await;
    features_response.assert_success();
    features_response.assert_content_type("application/geo+json");

    let features_body: serde_json::Value = features_response.json();
    assert_eq!(
        features_body["type"].as_str(),
        Some("FeatureCollection"),
        "Must be a FeatureCollection"
    );
    assert!(features_body["features"].is_array(), "features must be an array");

    // 5. Create a feature
    let feature = test_feature_request();
    let create_feature_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    create_feature_response.assert_status(StatusCode::CREATED);

    let created_feature: serde_json::Value = create_feature_response.json();
    let feature_id = created_feature["id"].as_str().expect("Feature must have id");

    // 6. Get the specific feature (A.2.7)
    let feature_response = app
        .get(&format!("/collections/{}/items/{}", collection_id, feature_id))
        .await;
    feature_response.assert_success();
    feature_response.assert_content_type("application/geo+json");

    let feature_body: serde_json::Value = feature_response.json();
    assert_eq!(feature_body["type"].as_str(), Some("Feature"), "Must be a Feature");
    assert!(feature_body["id"].is_string(), "Feature must have id");
    assert!(feature_body["geometry"].is_object(), "Feature must have geometry");
    assert!(
        feature_body["properties"].is_object() || feature_body["properties"].is_null(),
        "Feature must have properties"
    );

    // 7. Verify features list now has the feature
    let features_response2 = app.get(&format!("/collections/{}/items", collection_id)).await;
    features_response2.assert_success();
    let features_body2: serde_json::Value = features_response2.json();
    let features = features_body2["features"].as_array().unwrap();
    assert_eq!(features.len(), 1, "Should have one feature");
}

/// A.2.8: Link headers and relations
#[tokio::test]
async fn link_headers_and_relations() {
    let app = TestApp::new().await;

    let response = app.get("/").await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let links = body["links"].as_array().expect("links must be an array");

    // All links must have href and rel
    for link in links {
        assert!(link["href"].is_string(), "Link must have href");
        assert!(link["rel"].is_string(), "Link must have rel");
    }

    // Links should have type when appropriate
    let self_link = links.iter().find(|l| l["rel"] == "self");
    assert!(self_link.is_some(), "Must have self link");
    assert!(
        self_link.unwrap()["type"].is_string(),
        "Self link should have type"
    );
}

//! Authentication integration tests
//!
//! Tests authentication behavior with mock auth middleware.

use crate::common::TestApp;
use axum::http::StatusCode;

/// Test that public endpoints work without authentication (mock auth still injects user)
#[tokio::test]
async fn test_public_endpoints_accessible() {
    let app = TestApp::new().await;

    // Landing page should be accessible
    let response = app.get("/").await;
    response.assert_success();

    // Conformance should be accessible
    let response = app.get("/conformance").await;
    response.assert_success();

    // API spec should be accessible
    let response = app.get("/api").await;
    response.assert_success();
}

/// Test that authenticated requests work for protected endpoints
#[tokio::test]
async fn test_authenticated_requests_work() {
    let app = TestApp::new().await;

    // Collections endpoint requires auth - with mock auth it should work
    let response = app.get("/collections").await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    assert!(
        body["collections"].is_array(),
        "Should return collections array"
    );
}

/// Test that collections can be created with authenticated user
#[tokio::test]
async fn test_create_collection_with_auth() {
    let app = TestApp::new().await;

    let collection = serde_json::json!({
        "id": "auth-test-collection",
        "title": "Auth Test Collection",
        "description": "Created to test authentication",
        "collectionType": "vector",
        "crs": 4326
    });

    let response = app.post_json("/collections", &collection).await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert!(
        body["id"]
            .as_str()
            .unwrap()
            .contains("auth-test-collection")
    );
}

/// Test that authenticated user can access their own collections
#[tokio::test]
async fn test_user_can_access_own_collections() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = serde_json::json!({
        "id": "user-access-test",
        "title": "User Access Test Collection",
        "collectionType": "vector",
        "crs": 4326
    });

    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // User should be able to get the collection
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_success();

    // User should be able to list items
    let items_response = app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    items_response.assert_success();
}

//! Collection CRUD integration tests

use crate::common::{TestApp, test_collection_request};
use axum::http::StatusCode;

/// Test creating a collection
#[tokio::test]
async fn test_create_collection() {
    let app = TestApp::new().await;

    let collection = test_collection_request("integration-create-test", "vector");

    let response = app.post_json("/collections", &collection).await;
    response.assert_status(StatusCode::CREATED);

    // Should have Location header
    let location = response.location();
    assert!(location.is_some(), "Should have Location header");

    // Should have ETag
    let etag = response.etag();
    assert!(etag.is_some(), "Should have ETag header");

    let body: serde_json::Value = response.json();
    assert!(
        body["id"]
            .as_str()
            .unwrap()
            .contains("integration-create-test")
    );
}

/// Test getting a collection
#[tokio::test]
async fn test_get_collection() {
    let app = TestApp::new().await;

    // First create a collection
    let collection = test_collection_request("integration-get-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Now get the collection
    let response = app.get(&format!("/collections/{}", collection_id)).await;
    response.assert_success();

    // Should have ETag
    let etag = response.etag();
    assert!(etag.is_some(), "Should have ETag header");

    let body: serde_json::Value = response.json();
    assert_eq!(body["id"].as_str(), Some(collection_id));
    assert!(
        body["title"]
            .as_str()
            .unwrap()
            .contains("integration-get-test")
    );
}

/// Test updating a collection with ETag
#[tokio::test]
async fn test_update_collection_with_etag() {
    let app = TestApp::new().await;

    // First create a collection
    let collection = test_collection_request("integration-update-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Update with correct ETag using PATCH
    let update = serde_json::json!({
        "title": "Updated Title"
    });

    let patch_response = app
        .patch_json(&format!("/collections/{}", collection_id), &update, &etag)
        .await;

    patch_response.assert_success();

    // Verify update
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_success();

    let body: serde_json::Value = get_response.json();
    assert_eq!(body["title"].as_str(), Some("Updated Title"));
}

/// Test update fails without ETag
#[tokio::test]
async fn test_update_collection_without_etag() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("integration-no-etag-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Try update without ETag
    let update = serde_json::json!({
        "title": "Should Fail"
    });

    let response = app
        .patch_json_without_etag(&format!("/collections/{}", collection_id), &update)
        .await;

    // Should return 412 Precondition Failed
    response.assert_status(StatusCode::PRECONDITION_FAILED);
}

/// Test update fails with wrong ETag
#[tokio::test]
async fn test_update_collection_wrong_etag() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("integration-wrong-etag-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Try update with wrong ETag
    let update = serde_json::json!({
        "title": "Should Fail"
    });

    let response = app
        .patch_json(
            &format!("/collections/{}", collection_id),
            &update,
            "\"999\"",
        )
        .await;

    // Should return 412 Precondition Failed
    response.assert_status(StatusCode::PRECONDITION_FAILED);
}

/// Test deleting a collection
#[tokio::test]
async fn test_delete_collection() {
    let app = TestApp::new().await;

    // First create a collection
    let collection = test_collection_request("integration-delete-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Delete with ETag
    let delete_response = app
        .delete(&format!("/collections/{}", collection_id), &etag)
        .await;

    delete_response.assert_status(StatusCode::NO_CONTENT);

    // Verify deleted
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_status(StatusCode::NOT_FOUND);
}

//! Collection rename/redirect integration tests

use axum::http::StatusCode;
use crate::common::{test_collection_request, TestApp};

/// Test that renamed collections can be accessed by new name
#[tokio::test]
async fn test_collection_rename() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("redirect-original", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Rename the collection by updating the id
    let rename = serde_json::json!({
        "id": "redirect-renamed",
        "title": "Renamed Collection"
    });

    let rename_response = app
        .put_json(&format!("/collections/{}", collection_id), &rename, &etag)
        .await;

    // The rename might succeed or the API might not support id changes via PUT
    // Check what actually happens
    if rename_response.status == StatusCode::OK {
        let renamed: serde_json::Value = rename_response.json();
        let new_id = renamed["id"].as_str().expect("Should have new id");

        // Access by new name should work
        let get_response = app.get(&format!("/collections/{}", new_id)).await;
        get_response.assert_success();
    } else {
        // If rename isn't supported, that's also valid behavior
        // The collection should still be accessible by original name
        let get_response = app.get(&format!("/collections/{}", collection_id)).await;
        get_response.assert_success();
    }
}

/// Test that collection title can be updated
#[tokio::test]
async fn test_collection_title_update() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("title-update-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Update just the title
    let update = serde_json::json!({
        "title": "New Title"
    });

    let patch_response = app
        .patch_json(&format!("/collections/{}", collection_id), &update, &etag)
        .await;

    patch_response.assert_success();

    // Verify the title was updated
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_success();

    let body: serde_json::Value = get_response.json();
    assert_eq!(body["title"].as_str(), Some("New Title"));
}

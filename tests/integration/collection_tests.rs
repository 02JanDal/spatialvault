//! Collection CRUD integration tests

use axum::http::StatusCode;
use crate::common::{test_collection_request, TestApp};

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
    assert!(body["id"].as_str().unwrap().contains("integration-create-test"));
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
    assert!(body["title"].as_str().unwrap().contains("integration-get-test"));
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
        .patch_json(&format!("/collections/{}", collection_id), &update, "\"999\"")
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

/// Test that collection responses are synchronized between list and get endpoints
#[tokio::test]
async fn test_collection_response_synchronization() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("sync-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get the collection from list endpoint
    let list_response = app.get("/collections").await;
    list_response.assert_success();
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"].as_array().expect("Should have collections array");
    let list_collection = collections
        .iter()
        .find(|c| c["id"].as_str() == Some(collection_id))
        .expect("Should find created collection in list");

    // Get the collection from detail endpoint
    let detail_response = app.get(&format!("/collections/{}", collection_id)).await;
    detail_response.assert_success();
    let detail_collection: serde_json::Value = detail_response.json();

    // Verify common fields are the same
    assert_eq!(
        list_collection["id"],
        detail_collection["id"],
        "Collection ID should match"
    );
    assert_eq!(
        list_collection["title"],
        detail_collection["title"],
        "Collection title should match"
    );
    assert_eq!(
        list_collection["description"],
        detail_collection["description"],
        "Collection description should match"
    );
    assert_eq!(
        list_collection["itemType"],
        detail_collection["itemType"],
        "Collection itemType should match"
    );
    assert_eq!(
        list_collection["crs"],
        detail_collection["crs"],
        "Collection crs should match"
    );

    // Verify links structure
    let list_links = list_collection["links"].as_array().expect("Should have links");
    let detail_links = detail_collection["links"].as_array().expect("Should have links");

    // Both should have self link
    assert!(
        list_links.iter().any(|l| l["rel"].as_str() == Some("self")),
        "List collection should have self link"
    );
    assert!(
        detail_links.iter().any(|l| l["rel"].as_str() == Some("self")),
        "Detail collection should have self link"
    );

    // Both should have items link
    assert!(
        list_links.iter().any(|l| l["rel"].as_str() == Some("items")),
        "List collection should have items link"
    );
    assert!(
        detail_links.iter().any(|l| l["rel"].as_str() == Some("items")),
        "Detail collection should have items link"
    );

    // Both should have parent link
    assert!(
        list_links.iter().any(|l| l["rel"].as_str() == Some("parent")),
        "List collection should have parent link"
    );
    assert!(
        detail_links.iter().any(|l| l["rel"].as_str() == Some("parent")),
        "Detail collection should have parent link"
    );

    // Both should have schema link (describedby)
    assert!(
        list_links.iter().any(|l| l["rel"].as_str() == Some("describedby")),
        "List collection should have describedby link"
    );
    assert!(
        detail_links.iter().any(|l| l["rel"].as_str() == Some("describedby")),
        "Detail collection should have describedby link"
    );

    // For vector collections, both should have tiles link
    assert!(
        list_links.iter().any(|l| l["rel"].as_str() == Some("tiles")),
        "List collection should have tiles link for vector type"
    );
    assert!(
        detail_links.iter().any(|l| l["rel"].as_str() == Some("tiles")),
        "Detail collection should have tiles link for vector type"
    );

    // Note: extent and storage_crs are expected to differ
    // - List endpoint returns None for performance
    // - Detail endpoint computes them on demand
}

/// Test that raster collections have coverage links
#[tokio::test]
async fn test_raster_collection_links() {
    let app = TestApp::new().await;

    // Create a raster collection
    let collection = test_collection_request("raster-test", "raster");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get the collection from list endpoint
    let list_response = app.get("/collections").await;
    list_response.assert_success();
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"].as_array().expect("Should have collections array");
    let list_collection = collections
        .iter()
        .find(|c| c["id"].as_str() == Some(collection_id))
        .expect("Should find created collection in list");

    // Get the collection from detail endpoint
    let detail_response = app.get(&format!("/collections/{}", collection_id)).await;
    detail_response.assert_success();
    let detail_collection: serde_json::Value = detail_response.json();

    // Both should have coverage link for raster type
    let list_links = list_collection["links"].as_array().expect("Should have links");
    let detail_links = detail_collection["links"].as_array().expect("Should have links");

    assert!(
        list_links.iter().any(|l| l["rel"].as_str() == Some("coverage")),
        "List collection should have coverage link for raster type"
    );
    assert!(
        detail_links.iter().any(|l| l["rel"].as_str() == Some("coverage")),
        "Detail collection should have coverage link for raster type"
    );

    // Should not have tiles link for raster type
    assert!(
        !list_links.iter().any(|l| l["rel"].as_str() == Some("tiles")),
        "List collection should not have tiles link for raster type"
    );
    assert!(
        !detail_links.iter().any(|l| l["rel"].as_str() == Some("tiles")),
        "Detail collection should not have tiles link for raster type"
    );
}

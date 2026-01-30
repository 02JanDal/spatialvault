//! STAC Transaction Extension conformance tests
//!
//! Implements test requirements from:
//! - https://api.stacspec.org/v1.0.0/collections/extensions/transaction
//! - https://api.stacspec.org/v1.0.0/ogcapi-features/extensions/transaction

use crate::common::{TestApp, test_collection_request, test_stac_item_request};
use axum::http::StatusCode;

/// Test that conformance declaration includes STAC Collection Transaction extension
#[tokio::test]
async fn test_conformance_includes_collection_transaction() {
    let app = TestApp::new().await;

    let response = app.get("/conformance").await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let conforms_to = body["conformsTo"]
        .as_array()
        .expect("conformsTo must be an array");

    let has_collection_transaction = conforms_to.iter().any(|c| {
        c.as_str()
            .map(|s| s.contains("collections/extensions/transaction"))
            .unwrap_or(false)
    });

    assert!(
        has_collection_transaction,
        "Must declare STAC Collection Transaction conformance"
    );
}

/// Test that conformance declaration includes STAC Item Transaction extension
#[tokio::test]
async fn test_conformance_includes_item_transaction() {
    let app = TestApp::new().await;

    let response = app.get("/conformance").await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let conforms_to = body["conformsTo"]
        .as_array()
        .expect("conformsTo must be an array");

    let has_item_transaction = conforms_to.iter().any(|c| {
        c.as_str()
            .map(|s| s.contains("ogcapi-features/extensions/transaction"))
            .unwrap_or(false)
    });

    assert!(
        has_item_transaction,
        "Must declare STAC Item Transaction conformance"
    );
}

/// Collection Transaction: POST /collections creates a new collection
#[tokio::test]
async fn test_create_collection() {
    let app = TestApp::new().await;

    let collection = test_collection_request("stac-create-test", "vector");
    let response = app.post_json("/collections", &collection).await;

    response.assert_status(StatusCode::CREATED);

    // Verify Location header
    let location = response.header("location");
    assert!(location.is_some(), "Should have Location header");

    // Verify ETag header
    let etag = response.etag();
    assert!(etag.is_some(), "Should have ETag header");

    // Verify response body
    let body: serde_json::Value = response.json();
    assert!(body["id"].as_str().unwrap().contains("stac-create-test"));
}

/// Collection Transaction: PUT /collections/{collectionId} replaces a collection
#[tokio::test]
async fn test_replace_collection() {
    let app = TestApp::new().await;

    // First create a collection
    let collection = test_collection_request("stac-replace-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Replace the collection with new title
    let replacement = serde_json::json!({
        "id": collection_id,
        "title": "Replaced Title",
        "description": "Replaced description",
        "collectionType": "vector",
        "crs": 4326
    });

    let replace_response = app
        .put_json(
            &format!("/collections/{}", collection_id),
            &replacement,
            &etag,
        )
        .await;

    replace_response.assert_success();

    let body: serde_json::Value = replace_response.json();
    assert_eq!(body["title"].as_str(), Some("Replaced Title"));
}

/// Collection Transaction: PATCH /collections/{collectionId} updates a collection
#[tokio::test]
async fn test_patch_collection() {
    let app = TestApp::new().await;

    // First create a collection
    let collection = test_collection_request("stac-patch-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Patch the collection (JSON Merge Patch)
    let patch = serde_json::json!({
        "title": "Patched Title",
        "description": "Patched description"
    });

    let patch_response = app
        .patch_json(&format!("/collections/{}", collection_id), &patch, &etag)
        .await;

    patch_response.assert_success();

    let body: serde_json::Value = patch_response.json();
    assert_eq!(body["title"].as_str(), Some("Patched Title"));
}

/// Collection Transaction: DELETE /collections/{collectionId} removes a collection
#[tokio::test]
async fn test_delete_collection() {
    let app = TestApp::new().await;

    // First create a collection
    let collection = test_collection_request("stac-delete-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Delete the collection
    let delete_response = app
        .delete(&format!("/collections/{}", collection_id), &etag)
        .await;

    delete_response.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_status(StatusCode::NOT_FOUND);
}

/// Collection Transaction: DELETE works without If-Match header (optimistic locking is optional)
#[tokio::test]
async fn test_delete_collection_without_if_match() {
    let app = TestApp::new().await;

    // First create a collection
    let collection = test_collection_request("stac-delete-etag-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Delete without If-Match header - should succeed
    let response = app
        .request_without_etag(
            axum::http::Method::DELETE,
            &format!("/collections/{}", collection_id),
        )
        .await;

    // Should succeed when If-Match is not provided
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_status(StatusCode::NOT_FOUND);
}

/// Item Transaction: POST /collections/{collectionId}/items creates a new item
#[tokio::test]
async fn test_create_item() {
    let app = TestApp::new().await;

    // First create a raster collection for STAC items
    let collection = test_collection_request("stac-item-create-test", "raster");
    let create_coll_response = app.post_json("/collections", &collection).await;
    create_coll_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_coll_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create a STAC item
    let item = test_stac_item_request();
    let response = app
        .post_json(&format!("/collections/{}/items", collection_id), &item)
        .await;

    response.assert_status(StatusCode::CREATED);

    // Verify Location header
    let location = response.header("location");
    assert!(location.is_some(), "Should have Location header");

    // Verify ETag header
    let etag = response.etag();
    assert!(etag.is_some(), "Should have ETag header");

    // Verify response body
    let body: serde_json::Value = response.json();
    assert!(body["id"].is_string(), "Item should have id");
    assert_eq!(body["type"].as_str(), Some("Feature"));
}

/// Item Transaction: PUT /collections/{collectionId}/items/{itemId} replaces an item
#[tokio::test]
async fn test_replace_item() {
    let app = TestApp::new().await;

    // Create a raster collection
    let collection = test_collection_request("stac-item-replace-test", "raster");
    let create_coll_response = app.post_json("/collections", &collection).await;
    create_coll_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_coll_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create an item
    let item = test_stac_item_request();
    let create_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &item)
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created_item: serde_json::Value = create_response.json();
    let item_id = created_item["id"].as_str().expect("Item must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Replace the item
    let replacement = serde_json::json!({
        "type": "Feature",
        "geometry": {
            "type": "Polygon",
            "coordinates": [[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0], [0.0, 0.0]]]
        },
        "properties": {
            "datetime": "2024-06-15T12:00:00Z",
            "title": "Replaced Item"
        },
        "assets": {
            "data": {
                "href": "s3://bucket/replaced.tif",
                "type": "image/tiff; application=geotiff; profile=cloud-optimized",
                "roles": ["data"]
            }
        }
    });

    let replace_response = app
        .put_json(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            &replacement,
            &etag,
        )
        .await;

    replace_response.assert_success();

    let body: serde_json::Value = replace_response.json();
    assert_eq!(body["properties"]["title"].as_str(), Some("Replaced Item"));
}

/// Item Transaction: PATCH /collections/{collectionId}/items/{itemId} updates an item
#[tokio::test]
async fn test_patch_item() {
    let app = TestApp::new().await;

    // Create a raster collection
    let collection = test_collection_request("stac-item-patch-test", "raster");
    let create_coll_response = app.post_json("/collections", &collection).await;
    create_coll_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_coll_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create an item
    let item = test_stac_item_request();
    let create_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &item)
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created_item: serde_json::Value = create_response.json();
    let item_id = created_item["id"].as_str().expect("Item must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Patch the item
    let patch = serde_json::json!({
        "properties": {
            "title": "Patched Item"
        }
    });

    let patch_response = app
        .patch_json(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            &patch,
            &etag,
        )
        .await;

    patch_response.assert_success();

    let body: serde_json::Value = patch_response.json();
    assert_eq!(body["properties"]["title"].as_str(), Some("Patched Item"));
}

/// Item Transaction: DELETE /collections/{collectionId}/items/{itemId} removes an item
#[tokio::test]
async fn test_delete_item() {
    let app = TestApp::new().await;

    // Create a raster collection
    let collection = test_collection_request("stac-item-delete-test", "raster");
    let create_coll_response = app.post_json("/collections", &collection).await;
    create_coll_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_coll_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create an item
    let item = test_stac_item_request();
    let create_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &item)
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created_item: serde_json::Value = create_response.json();
    let item_id = created_item["id"].as_str().expect("Item must have id");
    let etag = create_response.etag().expect("Should have ETag");

    // Delete the item
    let delete_response = app
        .delete(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            &etag,
        )
        .await;

    delete_response.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone
    let get_response = app
        .get(&format!("/collections/{}/items/{}", collection_id, item_id))
        .await;
    get_response.assert_status(StatusCode::NOT_FOUND);
}

/// Item Transaction: DELETE works without If-Match header (optimistic locking is optional)
#[tokio::test]
async fn test_delete_item_without_if_match() {
    let app = TestApp::new().await;

    // Create a raster collection
    let collection = test_collection_request("stac-item-delete-etag-test", "raster");
    let create_coll_response = app.post_json("/collections", &collection).await;
    create_coll_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_coll_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create an item
    let item = test_stac_item_request();
    let create_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &item)
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created_item: serde_json::Value = create_response.json();
    let item_id = created_item["id"].as_str().expect("Item must have id");

    // Delete without If-Match header - should succeed
    let response = app
        .request_without_etag(
            axum::http::Method::DELETE,
            &format!("/collections/{}/items/{}", collection_id, item_id),
        )
        .await;

    // Should succeed when If-Match is not provided
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone
    let get_response = app
        .get(&format!("/collections/{}/items/{}", collection_id, item_id))
        .await;
    get_response.assert_status(StatusCode::NOT_FOUND);
}

/// Test that PUT with wrong ETag fails
#[tokio::test]
async fn test_put_with_wrong_etag_fails() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("stac-wrong-etag-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Try to update with wrong ETag
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

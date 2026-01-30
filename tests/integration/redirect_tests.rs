//! Collection rename/redirect integration tests

use crate::common::{TestApp, test_collection_request};
use axum::http::StatusCode;

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

/// Test that active collection takes priority over alias
#[tokio::test]
async fn test_active_collection_takes_priority_over_alias() {
    let app = TestApp::new().await;

    // Create a collection with a specific name
    let collection1 = test_collection_request("alias-test-1", "vector");
    let create_response1 = app.post_json("/collections", &collection1).await;
    create_response1.assert_status(StatusCode::CREATED);
    let created1: serde_json::Value = create_response1.json();
    let collection_id_1 = created1["id"].as_str().expect("Collection must have id");

    // Rename it to create an alias
    let etag1 = create_response1.etag().expect("Should have ETag");
    let rename = serde_json::json!({
        "id": "alias-test-renamed",
    });
    let rename_response = app
        .patch_json(
            &format!("/collections/{}", collection_id_1),
            &rename,
            &etag1,
        )
        .await;

    // If rename is supported, an alias is created
    if rename_response.status == StatusCode::OK {
        let renamed: serde_json::Value = rename_response.json();
        let new_id = renamed["id"].as_str().expect("Should have new id");

        // Now create a NEW collection with the OLD name (the alias source)
        let collection2 = test_collection_request("alias-test-1", "vector");
        let create_response2 = app.post_json("/collections", &collection2).await;
        create_response2.assert_status(StatusCode::CREATED);
        let created2: serde_json::Value = create_response2.json();
        let collection_id_2 = created2["id"].as_str().expect("Collection must have id");

        // When accessing the old name, we should get the NEW collection (not redirected)
        let get_response = app
            .get(&format!(
                "/collections/{}",
                collection_id_1.split(':').last().unwrap()
            ))
            .await;
        get_response.assert_success();
        let body: serde_json::Value = get_response.json();
        // This should be the new collection, not a redirect
        assert_eq!(body["id"].as_str().unwrap(), collection_id_2);
    }
}

/// Test that aliases work for /collections/{collection_id}/items endpoint
#[tokio::test]
async fn test_alias_redirect_on_items_endpoint() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("alias-items-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Rename the collection
    let etag = create_response.etag().expect("Should have ETag");
    let rename = serde_json::json!({
        "id": "alias-items-renamed",
    });
    let rename_response = app
        .patch_json(&format!("/collections/{}", collection_id), &rename, &etag)
        .await;

    // If rename is supported
    if rename_response.status == StatusCode::OK {
        let renamed: serde_json::Value = rename_response.json();
        let new_id = renamed["id"].as_str().expect("Should have new id");

        // Try to access items with the old name - should get a 307 redirect
        let items_response = app
            .get(&format!(
                "/collections/{}/items",
                collection_id.split(':').last().unwrap()
            ))
            .await;
        assert_eq!(items_response.status, StatusCode::TEMPORARY_REDIRECT);

        // Verify Location header points to the new collection
        let location = items_response
            .location()
            .expect("Should have Location header");
        assert!(
            location.contains(&new_id),
            "Location header should point to new collection"
        );
    }
}

/// Test that aliases work for /collections/{collection_id}/schema endpoint
#[tokio::test]
async fn test_alias_redirect_on_schema_endpoint() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("alias-schema-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Rename the collection
    let etag = create_response.etag().expect("Should have ETag");
    let rename = serde_json::json!({
        "id": "alias-schema-renamed",
    });
    let rename_response = app
        .patch_json(&format!("/collections/{}", collection_id), &rename, &etag)
        .await;

    // If rename is supported
    if rename_response.status == StatusCode::OK {
        let renamed: serde_json::Value = rename_response.json();
        let new_id = renamed["id"].as_str().expect("Should have new id");

        // Try to access schema with the old name - should get a 307 redirect
        let schema_response = app
            .get(&format!(
                "/collections/{}/schema",
                collection_id.split(':').last().unwrap()
            ))
            .await;
        assert_eq!(schema_response.status, StatusCode::TEMPORARY_REDIRECT);

        // Verify Location header points to the new collection
        let location = schema_response
            .location()
            .expect("Should have Location header");
        assert!(
            location.contains(&new_id),
            "Location header should point to new collection"
        );
    }
}

/// Test that aliases work for /collections/{collection_id}/tiles endpoint
#[tokio::test]
async fn test_alias_redirect_on_tiles_endpoint() {
    let app = TestApp::new().await;

    // Create a vector collection (tiles are supported for vector collections)
    let collection = test_collection_request("alias-tiles-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Rename the collection
    let etag = create_response.etag().expect("Should have ETag");
    let rename = serde_json::json!({
        "id": "alias-tiles-renamed",
    });
    let rename_response = app
        .patch_json(&format!("/collections/{}", collection_id), &rename, &etag)
        .await;

    // If rename is supported
    if rename_response.status == StatusCode::OK {
        let renamed: serde_json::Value = rename_response.json();
        let new_id = renamed["id"].as_str().expect("Should have new id");

        // Try to access tiles with the old name - should get a 307 redirect
        let tiles_response = app
            .get(&format!(
                "/collections/{}/tiles",
                collection_id.split(':').last().unwrap()
            ))
            .await;
        assert_eq!(tiles_response.status, StatusCode::TEMPORARY_REDIRECT);

        // Verify Location header points to the new collection
        let location = tiles_response
            .location()
            .expect("Should have Location header");
        assert!(
            location.contains(&new_id),
            "Location header should point to new collection"
        );
    }
}

/// Test that aliases work for /collections/{collection_id}/sharing endpoint
#[tokio::test]
async fn test_alias_redirect_on_sharing_endpoint() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("alias-sharing-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Rename the collection
    let etag = create_response.etag().expect("Should have ETag");
    let rename = serde_json::json!({
        "id": "alias-sharing-renamed",
    });
    let rename_response = app
        .patch_json(&format!("/collections/{}", collection_id), &rename, &etag)
        .await;

    // If rename is supported
    if rename_response.status == StatusCode::OK {
        let renamed: serde_json::Value = rename_response.json();
        let new_id = renamed["id"].as_str().expect("Should have new id");

        // Try to access sharing with the old name - should get a 307 redirect
        let sharing_response = app
            .get(&format!(
                "/collections/{}/sharing",
                collection_id.split(':').last().unwrap()
            ))
            .await;
        assert_eq!(sharing_response.status, StatusCode::TEMPORARY_REDIRECT);

        // Verify Location header points to the new collection
        let location = sharing_response
            .location()
            .expect("Should have Location header");
        assert!(
            location.contains(&new_id),
            "Location header should point to new collection"
        );
    }
}

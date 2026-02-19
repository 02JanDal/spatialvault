//! Authorization integration tests
//!
//! Tests for database-delegated authorization: owner access, non-owner isolation,
//! and shared access (read/write) for both vector and raster/pointcloud collections.

use crate::common::{
    MockAuthState, TestApp, test_collection_request, test_feature_request, test_stac_item_request,
};
use axum::http::StatusCode;

// ============================================================================
// Vector Collection Authorization Tests
// ============================================================================

/// Test that the owner has full access to their vector collection
#[tokio::test]
async fn test_owner_has_full_access_to_vector_collection() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a vector collection
    let collection = test_collection_request("owner-vector-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let collection_etag = create_response.etag().expect("Should have ETag");

    // Owner can list collections and see their collection
    let list_response = app.get("/collections").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"]
        .as_array()
        .expect("Should have collections");
    assert!(
        collections
            .iter()
            .any(|c| c["id"].as_str() == Some(collection_id)),
        "Owner should see their collection in list"
    );

    // Owner can GET the collection
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_status(StatusCode::OK);

    // Owner can GET items (empty list)
    let items_response = app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    items_response.assert_status(StatusCode::OK);

    // Owner can POST an item
    let feature = test_feature_request();
    let post_item_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    post_item_response.assert_status(StatusCode::CREATED);

    let item: serde_json::Value = post_item_response.json();
    let item_id = item["id"].as_str().expect("Item must have id");
    let item_etag = post_item_response.etag().expect("Should have ETag");

    // Owner can PATCH an item
    let update = serde_json::json!({
        "properties": {
            "name": "Updated Feature"
        }
    });
    let patch_item_response = app
        .patch_json(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            &update,
            &item_etag,
        )
        .await;
    patch_item_response.assert_status(StatusCode::OK);
    let updated_item_etag = patch_item_response.etag().expect("Should have ETag");

    // Owner can DELETE an item
    let delete_item_response = app
        .delete(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            &updated_item_etag,
        )
        .await;
    delete_item_response.assert_status(StatusCode::NO_CONTENT);

    // Get fresh collection ETag (item operations increment collection version)
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_status(StatusCode::OK);
    let fresh_collection_etag = get_response.etag().expect("Should have ETag");

    // Owner can PATCH the collection
    let collection_update = serde_json::json!({
        "title": "Updated Title"
    });
    let patch_collection_response = app
        .patch_json(
            &format!("/collections/{}", collection_id),
            &collection_update,
            &fresh_collection_etag,
        )
        .await;
    patch_collection_response.assert_status(StatusCode::OK);
}

/// Test that non-owners cannot access unshared vector collections (invisible - 404)
#[tokio::test]
async fn test_non_owner_cannot_access_vector_collection() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a vector collection as owner
    let collection = test_collection_request("non-owner-vector-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create a feature
    let feature = test_feature_request();
    let post_item_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    post_item_response.assert_status(StatusCode::CREATED);
    let item: serde_json::Value = post_item_response.json();
    let item_id = item["id"].as_str().expect("Item must have id");

    // Switch to a different user
    app.ensure_role_exists("otheruser").await;
    let other_app = app.spawn_user(MockAuthState::with_username("otheruser"));

    // Non-owner cannot see collection in list
    let list_response = other_app.get("/collections").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"]
        .as_array()
        .expect("Should have collections");
    assert!(
        !collections
            .iter()
            .any(|c| c["id"].as_str() == Some(collection_id)),
        "Non-owner should not see collection in list"
    );

    // Non-owner gets 404 on GET collection
    let get_response = other_app
        .get(&format!("/collections/{}", collection_id))
        .await;
    get_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on GET items
    let items_response = other_app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    items_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on POST item
    let feature = test_feature_request();
    let post_response = other_app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    post_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on PATCH item
    let update = serde_json::json!({ "properties": { "name": "Hacked" } });
    let patch_response = other_app
        .patch_json(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            &update,
            "\"1\"",
        )
        .await;
    patch_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on DELETE item
    let delete_response = other_app
        .delete(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            "\"1\"",
        )
        .await;
    delete_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on PATCH collection
    let collection_update = serde_json::json!({ "title": "Hacked Title" });
    let patch_collection_response = other_app
        .patch_json(
            &format!("/collections/{}", collection_id),
            &collection_update,
            "\"1\"",
        )
        .await;
    patch_collection_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on DELETE collection
    let delete_collection_response = other_app
        .delete(&format!("/collections/{}", collection_id), "\"1\"")
        .await;
    delete_collection_response.assert_status(StatusCode::NOT_FOUND);
}

/// Test that read share grants read-only access for vector collections
#[tokio::test]
async fn test_read_share_grants_read_only_access_vector() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a vector collection as owner
    let collection = test_collection_request("read-share-vector-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create a feature
    let feature = test_feature_request();
    let post_item_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    post_item_response.assert_status(StatusCode::CREATED);
    let item: serde_json::Value = post_item_response.json();
    let item_id = item["id"].as_str().expect("Item must have id");

    // Share with read permission
    app.ensure_role_exists("reader").await;
    let share_request = serde_json::json!({
        "principal": "reader",
        "principal_type": "user",
        "permission": "read"
    });
    let share_response = app
        .post_json(
            &format!("/collections/{}/sharing", collection_id),
            &share_request,
        )
        .await;
    share_response.assert_status(StatusCode::CREATED);

    // Switch to reader user
    let reader_app = app.spawn_user(MockAuthState::with_username("reader"));

    // Reader can see collection in list
    let list_response = reader_app.get("/collections").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"]
        .as_array()
        .expect("Should have collections");
    assert!(
        collections
            .iter()
            .any(|c| c["id"].as_str() == Some(collection_id)),
        "Reader should see collection in list"
    );

    // Reader can GET the collection
    let get_response = reader_app
        .get(&format!("/collections/{}", collection_id))
        .await;
    get_response.assert_status(StatusCode::OK);

    // Reader can GET items
    let items_response = reader_app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    items_response.assert_status(StatusCode::OK);

    // Reader gets 403 on POST item
    let feature = test_feature_request();
    let post_response = reader_app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    post_response.assert_status(StatusCode::FORBIDDEN);

    // Reader gets 403 on PATCH item
    let update = serde_json::json!({ "properties": { "name": "Updated" } });
    let patch_response = reader_app
        .patch_json(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            &update,
            "\"1\"",
        )
        .await;
    patch_response.assert_status(StatusCode::FORBIDDEN);

    // Reader gets 403 on DELETE item
    let delete_response = reader_app
        .delete(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            "\"1\"",
        )
        .await;
    delete_response.assert_status(StatusCode::FORBIDDEN);

    // Reader gets 403 on PATCH collection
    let collection_update = serde_json::json!({ "title": "Updated Title" });
    let patch_collection_response = reader_app
        .patch_json(
            &format!("/collections/{}", collection_id),
            &collection_update,
            "\"1\"",
        )
        .await;
    patch_collection_response.assert_status(StatusCode::FORBIDDEN);

    // Reader gets 403 on DELETE collection
    let delete_collection_response = reader_app
        .delete(&format!("/collections/{}", collection_id), "\"1\"")
        .await;
    delete_collection_response.assert_status(StatusCode::FORBIDDEN);
}

/// Test that write share grants item modification but not collection modification
#[tokio::test]
async fn test_write_share_grants_item_modification_vector() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a vector collection as owner
    let collection = test_collection_request("write-share-vector-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Share with write permission
    app.ensure_role_exists("writer").await;
    let share_request = serde_json::json!({
        "principal": "writer",
        "principal_type": "user",
        "permission": "write"
    });
    let share_response = app
        .post_json(
            &format!("/collections/{}/sharing", collection_id),
            &share_request,
        )
        .await;
    share_response.assert_status(StatusCode::CREATED);

    // Switch to writer user
    let writer_app = app.spawn_user(MockAuthState::with_username("writer"));

    // Writer can see collection in list
    let list_response = writer_app.get("/collections").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"]
        .as_array()
        .expect("Should have collections");
    assert!(
        collections
            .iter()
            .any(|c| c["id"].as_str() == Some(collection_id)),
        "Writer should see collection in list"
    );

    // Writer can GET the collection
    let get_response = writer_app
        .get(&format!("/collections/{}", collection_id))
        .await;
    get_response.assert_status(StatusCode::OK);

    // Writer can GET items
    let items_response = writer_app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    items_response.assert_status(StatusCode::OK);

    // Writer can POST an item
    let feature = test_feature_request();
    let post_response = writer_app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    post_response.assert_status(StatusCode::CREATED);

    let item: serde_json::Value = post_response.json();
    let item_id = item["id"].as_str().expect("Item must have id");
    let item_etag = post_response.etag().expect("Should have ETag");

    // Writer can PATCH an item
    let update = serde_json::json!({ "properties": { "name": "Updated by Writer" } });
    let patch_response = writer_app
        .patch_json(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            &update,
            &item_etag,
        )
        .await;
    patch_response.assert_status(StatusCode::OK);
    let updated_item_etag = patch_response.etag().expect("Should have ETag");

    // Writer can DELETE an item
    let delete_response = writer_app
        .delete(
            &format!("/collections/{}/items/{}", collection_id, item_id),
            &updated_item_etag,
        )
        .await;
    delete_response.assert_status(StatusCode::NO_CONTENT);

    // Writer gets 403 on PATCH collection
    let collection_update = serde_json::json!({ "title": "Updated Title" });
    let patch_collection_response = writer_app
        .patch_json(
            &format!("/collections/{}", collection_id),
            &collection_update,
            "\"1\"",
        )
        .await;
    patch_collection_response.assert_status(StatusCode::FORBIDDEN);

    // Writer gets 403 on DELETE collection
    let delete_collection_response = writer_app
        .delete(&format!("/collections/{}", collection_id), "\"1\"")
        .await;
    delete_collection_response.assert_status(StatusCode::FORBIDDEN);
}

// ============================================================================
// Raster Collection Authorization Tests
// ============================================================================

/// Test that the owner has full access to their raster collection
#[tokio::test]
async fn test_owner_has_full_access_to_raster_collection() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a raster collection
    let collection = test_collection_request("owner-raster-test", "raster");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let collection_etag = create_response.etag().expect("Should have ETag");

    // Owner can list collections and see their collection
    let list_response = app.get("/collections").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"]
        .as_array()
        .expect("Should have collections");
    assert!(
        collections
            .iter()
            .any(|c| c["id"].as_str() == Some(collection_id)),
        "Owner should see their collection in list"
    );

    // Owner can GET the collection
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_status(StatusCode::OK);

    // Owner can GET items (empty list)
    let items_response = app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    items_response.assert_status(StatusCode::OK);

    // POST items is not supported for raster collections (use processes API)
    let stac_item = test_stac_item_request();
    let post_item_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &stac_item)
        .await;
    post_item_response.assert_status(StatusCode::BAD_REQUEST);

    // Owner can PATCH the collection
    let collection_update = serde_json::json!({
        "title": "Updated Raster Title"
    });
    let patch_collection_response = app
        .patch_json(
            &format!("/collections/{}", collection_id),
            &collection_update,
            &collection_etag,
        )
        .await;
    patch_collection_response.assert_status(StatusCode::OK);
}

/// Test that non-owners cannot access unshared raster collections (invisible - 404)
#[tokio::test]
async fn test_non_owner_cannot_access_raster_collection() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a raster collection as owner
    let collection = test_collection_request("non-owner-raster-test", "raster");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Switch to a different user
    app.ensure_role_exists("otheruser").await;
    let other_app = app.spawn_user(MockAuthState::with_username("otheruser"));

    // Non-owner cannot see collection in list
    let list_response = other_app.get("/collections").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"]
        .as_array()
        .expect("Should have collections");
    assert!(
        !collections
            .iter()
            .any(|c| c["id"].as_str() == Some(collection_id)),
        "Non-owner should not see collection in list"
    );

    // Non-owner gets 404 on GET collection
    let get_response = other_app
        .get(&format!("/collections/{}", collection_id))
        .await;
    get_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on GET items
    let items_response = other_app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    items_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on POST item
    let stac_item = test_stac_item_request();
    let post_response = other_app
        .post_json(&format!("/collections/{}/items", collection_id), &stac_item)
        .await;
    post_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on PATCH collection
    let collection_update = serde_json::json!({ "title": "Hacked Title" });
    let patch_collection_response = other_app
        .patch_json(
            &format!("/collections/{}", collection_id),
            &collection_update,
            "\"1\"",
        )
        .await;
    patch_collection_response.assert_status(StatusCode::NOT_FOUND);

    // Non-owner gets 404 on DELETE collection
    let delete_collection_response = other_app
        .delete(&format!("/collections/{}", collection_id), "\"1\"")
        .await;
    delete_collection_response.assert_status(StatusCode::NOT_FOUND);
}

/// Test that read share grants read-only access for raster collections
#[tokio::test]
async fn test_read_share_grants_read_only_access_raster() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a raster collection as owner
    let collection = test_collection_request("read-share-raster-test", "raster");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Share with read permission
    app.ensure_role_exists("reader").await;
    let share_request = serde_json::json!({
        "principal": "reader",
        "principal_type": "user",
        "permission": "read"
    });
    let share_response = app
        .post_json(
            &format!("/collections/{}/sharing", collection_id),
            &share_request,
        )
        .await;
    share_response.assert_status(StatusCode::CREATED);

    // Switch to reader user
    let reader_app = app.spawn_user(MockAuthState::with_username("reader"));

    // Reader can see collection in list
    let list_response = reader_app.get("/collections").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"]
        .as_array()
        .expect("Should have collections");
    assert!(
        collections
            .iter()
            .any(|c| c["id"].as_str() == Some(collection_id)),
        "Reader should see collection in list"
    );

    // Reader can GET the collection
    let get_response = reader_app
        .get(&format!("/collections/{}", collection_id))
        .await;
    get_response.assert_status(StatusCode::OK);

    // Reader can GET items
    let items_response = reader_app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    items_response.assert_status(StatusCode::OK);

    // Reader gets 403 on PATCH collection
    let collection_update = serde_json::json!({ "title": "Updated Title" });
    let patch_collection_response = reader_app
        .patch_json(
            &format!("/collections/{}", collection_id),
            &collection_update,
            "\"1\"",
        )
        .await;
    patch_collection_response.assert_status(StatusCode::FORBIDDEN);

    // Reader gets 403 on DELETE collection
    let delete_collection_response = reader_app
        .delete(&format!("/collections/{}", collection_id), "\"1\"")
        .await;
    delete_collection_response.assert_status(StatusCode::FORBIDDEN);
}

/// Test that write share grants access but not collection modification for raster
#[tokio::test]
async fn test_write_share_grants_item_modification_raster() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a raster collection as owner
    let collection = test_collection_request("write-share-raster-test", "raster");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Share with write permission
    app.ensure_role_exists("writer").await;
    let share_request = serde_json::json!({
        "principal": "writer",
        "principal_type": "user",
        "permission": "write"
    });
    let share_response = app
        .post_json(
            &format!("/collections/{}/sharing", collection_id),
            &share_request,
        )
        .await;
    share_response.assert_status(StatusCode::CREATED);

    // Switch to writer user
    let writer_app = app.spawn_user(MockAuthState::with_username("writer"));

    // Writer can see collection in list
    let list_response = writer_app.get("/collections").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"]
        .as_array()
        .expect("Should have collections");
    assert!(
        collections
            .iter()
            .any(|c| c["id"].as_str() == Some(collection_id)),
        "Writer should see collection in list"
    );

    // Writer can GET the collection
    let get_response = writer_app
        .get(&format!("/collections/{}", collection_id))
        .await;
    get_response.assert_status(StatusCode::OK);

    // Writer can GET items
    let items_response = writer_app
        .get(&format!("/collections/{}/items", collection_id))
        .await;
    items_response.assert_status(StatusCode::OK);

    // Writer gets 403 on PATCH collection
    let collection_update = serde_json::json!({ "title": "Updated Title" });
    let patch_collection_response = writer_app
        .patch_json(
            &format!("/collections/{}", collection_id),
            &collection_update,
            "\"1\"",
        )
        .await;
    patch_collection_response.assert_status(StatusCode::FORBIDDEN);

    // Writer gets 403 on DELETE collection
    let delete_collection_response = writer_app
        .delete(&format!("/collections/{}", collection_id), "\"1\"")
        .await;
    delete_collection_response.assert_status(StatusCode::FORBIDDEN);
}

// ============================================================================
// Share Revocation and Owner-Only Tests
// ============================================================================

/// Test that share revocation removes access
#[tokio::test]
async fn test_share_revocation_removes_access() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a vector collection
    let collection = test_collection_request("revoke-share-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Share with read permission
    app.ensure_role_exists("tempuser").await;
    let share_request = serde_json::json!({
        "principal": "tempuser",
        "principal_type": "user",
        "permission": "read"
    });
    let share_response = app
        .post_json(
            &format!("/collections/{}/sharing", collection_id),
            &share_request,
        )
        .await;
    share_response.assert_status(StatusCode::CREATED);

    // Switch to tempuser
    let temp_app = app.spawn_user(MockAuthState::with_username("tempuser"));

    // tempuser can access the collection
    let get_response = temp_app
        .get(&format!("/collections/{}", collection_id))
        .await;
    get_response.assert_status(StatusCode::OK);

    // Owner revokes the share
    let revoke_response = app
        .request_without_etag(
            axum::http::Method::DELETE,
            &format!("/collections/{}/sharing/tempuser", collection_id),
        )
        .await;
    revoke_response.assert_status(StatusCode::NO_CONTENT);

    // tempuser can no longer access the collection (gets 404)
    let get_response_after = temp_app
        .get(&format!("/collections/{}", collection_id))
        .await;
    get_response_after.assert_status(StatusCode::NOT_FOUND);

    // tempuser no longer sees collection in list
    let list_response = temp_app.get("/collections").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let collections = list_body["collections"]
        .as_array()
        .expect("Should have collections");
    assert!(
        !collections
            .iter()
            .any(|c| c["id"].as_str() == Some(collection_id)),
        "Collection should be invisible after share revocation"
    );
}

/// Test that only the owner can delete a collection
#[tokio::test]
async fn test_only_owner_can_delete_collection() {
    let app = TestApp::with_auth(MockAuthState::with_username("owner")).await;

    // Create a vector collection
    let collection = test_collection_request("owner-delete-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");
    let collection_etag = create_response.etag().expect("Should have ETag");

    // Share with write permission (highest non-owner permission)
    app.ensure_role_exists("writer").await;
    let share_request = serde_json::json!({
        "principal": "writer",
        "principal_type": "user",
        "permission": "write"
    });
    let share_response = app
        .post_json(
            &format!("/collections/{}/sharing", collection_id),
            &share_request,
        )
        .await;
    share_response.assert_status(StatusCode::CREATED);

    // Switch to writer user
    let writer_app = app.spawn_user(MockAuthState::with_username("writer"));

    // Writer gets 403 on DELETE collection (even with write permission)
    let delete_response = writer_app
        .delete(&format!("/collections/{}", collection_id), &collection_etag)
        .await;
    delete_response.assert_status(StatusCode::FORBIDDEN);

    // Owner can delete the collection
    let owner_delete_response = app
        .delete(&format!("/collections/{}", collection_id), &collection_etag)
        .await;
    owner_delete_response.assert_status(StatusCode::NO_CONTENT);

    // Verify deleted
    let get_response = app.get(&format!("/collections/{}", collection_id)).await;
    get_response.assert_status(StatusCode::NOT_FOUND);
}

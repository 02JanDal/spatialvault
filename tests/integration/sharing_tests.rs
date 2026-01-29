//! Sharing and permissions integration tests

use axum::http::StatusCode;
use crate::common::{test_collection_request, TestApp};

/// Test listing shares for a collection
#[tokio::test]
async fn test_list_shares() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("sharing-list-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Get shares
    let response = app
        .get(&format!("/collections/{}/sharing", collection_id))
        .await;

    response.assert_success();

    let body: serde_json::Value = response.json();
    assert!(body["shares"].is_array(), "Should have shares array");
    assert!(
        body["collection_id"].is_string(),
        "Should have collection_id"
    );
}

/// Test adding a share
#[tokio::test]
async fn test_add_share() {
    let app = TestApp::new().await;

    // Ensure the target user exists before sharing
    app.ensure_role_exists("otheruser").await;

    // Create a collection
    let collection = test_collection_request("sharing-add-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Add share
    let share_request = serde_json::json!({
        "principal": "otheruser",
        "principal_type": "user",
        "permission": "read"
    });

    let response = app
        .post_json(&format!("/collections/{}/sharing", collection_id), &share_request)
        .await;

    // Should succeed since testuser owns the collection
    response.assert_status(StatusCode::CREATED);

    // Verify share appears in list
    let list_response = app
        .get(&format!("/collections/{}/sharing", collection_id))
        .await;
    list_response.assert_success();

    let list_body: serde_json::Value = list_response.json();
    let shares = list_body["shares"].as_array().expect("Should have shares");

    let has_share = shares.iter().any(|s| {
        s["principal"].as_str() == Some("otheruser")
    });
    assert!(has_share, "Share should appear in list");
}

/// Test removing a share
#[tokio::test]
async fn test_remove_share() {
    let app = TestApp::new().await;

    // Ensure the target user exists before sharing
    app.ensure_role_exists("shareuser").await;

    // Create a collection
    let collection = test_collection_request("sharing-remove-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Add share first
    let share_request = serde_json::json!({
        "principal": "shareuser",
        "principal_type": "user",
        "permission": "read"
    });

    let add_response = app
        .post_json(&format!("/collections/{}/sharing", collection_id), &share_request)
        .await;
    add_response.assert_status(StatusCode::CREATED);

    // Remove the share (no ETag needed for shares)
    let remove_response = app
        .request_without_etag(
            axum::http::Method::DELETE,
            &format!("/collections/{}/sharing/shareuser", collection_id),
        )
        .await;

    remove_response.assert_status(StatusCode::NO_CONTENT);

    // Verify share is gone
    let list_response = app
        .get(&format!("/collections/{}/sharing", collection_id))
        .await;
    list_response.assert_success();

    let list_body: serde_json::Value = list_response.json();
    let shares = list_body["shares"].as_array().expect("Should have shares");

    let has_share = shares.iter().any(|s| {
        s["principal"].as_str() == Some("shareuser")
    });
    assert!(!has_share, "Share should be removed");
}

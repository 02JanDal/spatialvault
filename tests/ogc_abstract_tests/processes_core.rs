//! OGC API Processes Core conformance tests
//!
//! Implements abstract test requirements from:
//! http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/core

use axum::http::StatusCode;
use crate::common::{test_collection_request, TestApp};

/// Test process list endpoint
#[tokio::test]
async fn test_process_list() {
    let app = TestApp::new().await;

    let response = app.get("/processes").await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    // Must have processes array
    let processes = body["processes"]
        .as_array()
        .expect("processes must be an array");

    // Should have import-raster and import-pointcloud
    let process_ids: Vec<&str> = processes
        .iter()
        .filter_map(|p| p["id"].as_str())
        .collect();

    assert!(
        process_ids.contains(&"import-raster"),
        "Should have import-raster process"
    );
    assert!(
        process_ids.contains(&"import-pointcloud"),
        "Should have import-pointcloud process"
    );
}

/// Test process description endpoint
#[tokio::test]
async fn test_process_description() {
    let app = TestApp::new().await;

    let response = app.get("/processes/import-raster").await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    // Must have required properties
    assert!(body["id"].is_string(), "Process must have id");
    assert!(body["title"].is_string(), "Process must have title");
    assert!(body["version"].is_string(), "Process must have version");
    assert!(
        body["jobControlOptions"].is_array(),
        "Process must have jobControlOptions"
    );
    assert!(body["inputs"].is_object(), "Process must have inputs");
    assert!(body["outputs"].is_object(), "Process must have outputs");
}

/// Test process not found returns 404
#[tokio::test]
async fn test_process_not_found() {
    let app = TestApp::new().await;

    let response = app.get("/processes/nonexistent-process").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

/// Test job execution (async)
#[tokio::test]
async fn test_job_execution() {
    let app = TestApp::new().await;

    // First create a collection for the job
    let collection = test_collection_request("job-test", "raster");
    let create_coll_response = app.post_json("/collections", &collection).await;
    create_coll_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_coll_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Use the correct input format with "data" as a reference value
    let inputs = serde_json::json!({
        "collection": collection_id,
        "data": {
            "href": "s3://test-bucket/test.tif"
        }
    });

    let response = app
        .post_json(
            "/processes/import-raster/execution",
            &serde_json::json!({ "inputs": inputs }),
        )
        .await;

    // Should return 201 Created with job info
    response.assert_status(StatusCode::CREATED);

    // Should have Location header
    let location = response.header("location");
    assert!(location.is_some(), "Should have Location header");

    let body: serde_json::Value = response.json();

    assert!(body["jobId"].is_string(), "Should have jobId");
    assert!(body["status"].is_string(), "Should have status");
    assert_eq!(
        body["status"].as_str(),
        Some("accepted"),
        "Initial status should be accepted"
    );
}

/// Test job list endpoint
#[tokio::test]
async fn test_job_list() {
    let app = TestApp::new().await;

    let response = app.get("/jobs").await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    assert!(body["jobs"].is_array(), "Should have jobs array");
    assert!(body["links"].is_array(), "Should have links");
}

/// Test job status endpoint
#[tokio::test]
async fn test_job_status() {
    let app = TestApp::new().await;

    // First create a collection for the job
    let collection = test_collection_request("job-status-test", "raster");
    let create_coll_response = app.post_json("/collections", &collection).await;
    create_coll_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_coll_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Use the correct input format with "data" as a reference value
    let inputs = serde_json::json!({
        "collection": collection_id,
        "data": {
            "href": "s3://test-bucket/test.tif"
        }
    });

    let create_response = app
        .post_json(
            "/processes/import-raster/execution",
            &serde_json::json!({ "inputs": inputs }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = create_response.json();
    let job_id = create_body["jobId"]
        .as_str()
        .expect("Should have jobId");

    // Get job status
    let response = app.get(&format!("/jobs/{}", job_id)).await;
    response.assert_success();
    response.assert_content_type("application/json");

    let body: serde_json::Value = response.json();

    assert_eq!(body["jobId"].as_str(), Some(job_id));
    assert!(body["status"].is_string(), "Should have status");
    assert!(body["processId"].is_string(), "Should have processId");

    // Status should be one of the valid values
    let status = body["status"].as_str().unwrap();
    let valid_statuses = ["accepted", "running", "successful", "failed", "dismissed"];
    assert!(
        valid_statuses.contains(&status),
        "Status should be valid: {}",
        status
    );
}

/// Test job dismiss (cancel)
#[tokio::test]
async fn test_job_dismiss() {
    let app = TestApp::new().await;

    // First create a collection for the job
    let collection = test_collection_request("job-dismiss-test", "raster");
    let create_coll_response = app.post_json("/collections", &collection).await;
    create_coll_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_coll_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Use the correct input format with "data" as a reference value
    let inputs = serde_json::json!({
        "collection": collection_id,
        "data": {
            "href": "s3://test-bucket/test.tif"
        }
    });

    let create_response = app
        .post_json(
            "/processes/import-raster/execution",
            &serde_json::json!({ "inputs": inputs }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = create_response.json();
    let job_id = create_body["jobId"]
        .as_str()
        .expect("Should have jobId");

    // Dismiss the job (we need a delete method without ETag for jobs)
    let response = app
        .request_without_etag(axum::http::Method::DELETE, &format!("/jobs/{}", job_id))
        .await;

    // Should return 200 OK or 204 No Content
    assert!(
        response.status == StatusCode::OK || response.status == StatusCode::NO_CONTENT,
        "Should return 200 or 204, got: {}",
        response.status
    );

    // Verify job is dismissed
    let status_response = app.get(&format!("/jobs/{}", job_id)).await;
    status_response.assert_success();

    let status_body: serde_json::Value = status_response.json();
    assert_eq!(
        status_body["status"].as_str(),
        Some("dismissed"),
        "Job should be dismissed"
    );
}

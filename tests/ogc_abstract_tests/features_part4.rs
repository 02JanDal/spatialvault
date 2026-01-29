//! OGC API Features Part 4 conformance tests
//!
//! Implements test requirements from:
//! - http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/create-replace-delete
//! - http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/update
//! - http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/optimistic-locking-etags

use crate::common::TestApp;

/// Test that conformance declaration includes create-replace-delete class
#[tokio::test]
async fn test_conformance_includes_create_replace_delete() {
    let app = TestApp::new().await;

    let response = app.get("/conformance").await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let conforms_to = body["conformsTo"]
        .as_array()
        .expect("conformsTo must be an array");

    let has_create_replace_delete = conforms_to.iter().any(|c| {
        c.as_str()
            .map(|s| s == "http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/create-replace-delete")
            .unwrap_or(false)
    });

    assert!(
        has_create_replace_delete,
        "Must declare OGC API Features Part 4 create-replace-delete conformance"
    );
}

/// Test that conformance declaration includes update class
#[tokio::test]
async fn test_conformance_includes_update() {
    let app = TestApp::new().await;

    let response = app.get("/conformance").await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let conforms_to = body["conformsTo"]
        .as_array()
        .expect("conformsTo must be an array");

    let has_update = conforms_to.iter().any(|c| {
        c.as_str()
            .map(|s| s == "http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/update")
            .unwrap_or(false)
    });

    assert!(
        has_update,
        "Must declare OGC API Features Part 4 update conformance"
    );
}

/// Test that conformance declaration includes optimistic locking with ETags
#[tokio::test]
async fn test_conformance_includes_optimistic_locking_etags() {
    let app = TestApp::new().await;

    let response = app.get("/conformance").await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let conforms_to = body["conformsTo"]
        .as_array()
        .expect("conformsTo must be an array");

    let has_optimistic_locking = conforms_to.iter().any(|c| {
        c.as_str()
            .map(|s| s == "http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/optimistic-locking-etags")
            .unwrap_or(false)
    });

    assert!(
        has_optimistic_locking,
        "Must declare OGC API Features Part 4 optimistic-locking-etags conformance"
    );
}

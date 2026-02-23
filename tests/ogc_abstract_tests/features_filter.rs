//! OGC API Features Filter conformance tests
//!
//! Implements abstract test requirements from:
//! http://www.opengis.net/spec/ogcapi-features-3/1.0/conf/filter
//!
//! Reference: https://docs.ogc.org/is/19-079r2/19-079r2.html#_requirements_class_filter

use crate::common::{TestApp, test_collection_request};
use axum::http::StatusCode;
use spatialvault::api::conformance::classes;

/// A.3.1: Conformance declaration includes the Filter class
#[tokio::test]
async fn filter_conformance_declared() {
    let app = TestApp::new().await;

    let response = app.get("/conformance").await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let conforms_to = body["conformsTo"]
        .as_array()
        .expect("conformsTo must be an array");

    let has_filter = conforms_to
        .iter()
        .any(|c| c.as_str() == Some(classes::FEATURES_FILTER));

    assert!(
        has_filter,
        "Conformance declaration must include the Filter class ({})",
        classes::FEATURES_FILTER
    );
}

/// A.3.2: filter parameter with cql2-text is accepted
#[tokio::test]
async fn filter_parameter_cql2_text_accepted() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("filter-text-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create a feature
    let feature = serde_json::json!({
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "Berlin", "value": 100 }
    });
    let feature_response = app
        .post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await;
    feature_response.assert_status(StatusCode::CREATED);

    // Apply a CQL2 text filter using filter-lang=cql2-text
    let response = app
        .get(&format!(
            "/collections/{}/items?filter=name='Berlin'&filter-lang=cql2-text",
            collection_id
        ))
        .await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let features = body["features"].as_array().expect("features must be array");
    assert_eq!(
        features.len(),
        1,
        "Filter should return exactly one matching feature"
    );
    assert_eq!(
        features[0]["properties"]["name"].as_str(),
        Some("Berlin"),
        "Returned feature must match filter"
    );
}

/// A.3.3: filter-lang defaults to cql2-text when omitted
#[tokio::test]
async fn filter_lang_defaults_to_cql2_text() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("filter-default-lang", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create features
    let feature1 = serde_json::json!({
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [2.35, 48.86] },
        "properties": { "name": "Paris", "value": 200 }
    });
    let feature2 = serde_json::json!({
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [13.4, 52.5] },
        "properties": { "name": "Berlin", "value": 300 }
    });
    app.post_json(&format!("/collections/{}/items", collection_id), &feature1)
        .await
        .assert_status(StatusCode::CREATED);
    app.post_json(&format!("/collections/{}/items", collection_id), &feature2)
        .await
        .assert_status(StatusCode::CREATED);

    // Filter without specifying filter-lang (should default to cql2-text)
    // Note: '>' must be percent-encoded as '%3E' in a URI
    let response = app
        .get(&format!(
            "/collections/{}/items?filter=value%3E200",
            collection_id
        ))
        .await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let features = body["features"].as_array().expect("features must be array");
    assert_eq!(
        features.len(),
        1,
        "Filter value>200 should return only the Berlin feature"
    );
}

/// A.3.4: filter parameter with cql2-json is accepted
#[tokio::test]
async fn filter_parameter_cql2_json_accepted() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("filter-json-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create features
    let feature1 = serde_json::json!({
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [2.35, 48.86] },
        "properties": { "name": "Paris", "value": 200 }
    });
    let feature2 = serde_json::json!({
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [13.4, 52.5] },
        "properties": { "name": "Berlin", "value": 300 }
    });
    app.post_json(&format!("/collections/{}/items", collection_id), &feature1)
        .await
        .assert_status(StatusCode::CREATED);
    app.post_json(&format!("/collections/{}/items", collection_id), &feature2)
        .await
        .assert_status(StatusCode::CREATED);

    // CQL2-JSON filter expression (percent-encoded)
    // {"op":"=","args":[{"property":"name"},"Paris"]}
    let encoded_filter = "%7B%22op%22%3A%22%3D%22%2C%22args%22%3A%5B%7B%22property%22%3A%22name%22%7D%2C%22Paris%22%5D%7D";

    let response = app
        .get(&format!(
            "/collections/{}/items?filter={}&filter-lang=cql2-json",
            collection_id, encoded_filter
        ))
        .await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let features = body["features"].as_array().expect("features must be array");
    assert_eq!(
        features.len(),
        1,
        "CQL2-JSON filter should return exactly one matching feature"
    );
    assert_eq!(
        features[0]["properties"]["name"].as_str(),
        Some("Paris"),
        "Returned feature must match the CQL2-JSON filter"
    );
}

/// A.3.5: CQL2 filter with logical operator (AND)
#[tokio::test]
async fn filter_logical_and() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("filter-logical-and", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create features
    for (name, value) in [("Alpha", 10), ("Beta", 20), ("Alpha", 30)] {
        let feature = serde_json::json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
            "properties": { "name": name, "value": value }
        });
        app.post_json(&format!("/collections/{}/items", collection_id), &feature)
            .await
            .assert_status(StatusCode::CREATED);
    }

    // Filter with AND: name='Alpha' AND value>15
    // Spaces and '>' must be percent-encoded in a URI
    let response = app
        .get(&format!(
            "/collections/{}/items?filter=name%3D'Alpha'%20AND%20value%3E15&filter-lang=cql2-text",
            collection_id
        ))
        .await;
    response.assert_success();

    let body: serde_json::Value = response.json();
    let features = body["features"].as_array().expect("features must be array");
    assert_eq!(
        features.len(),
        1,
        "AND filter should return only the Alpha feature with value>15"
    );
    assert_eq!(
        features[0]["properties"]["value"].as_i64(),
        Some(30),
        "Should return the Alpha feature with value 30"
    );
}

/// A.3.6: Invalid filter expression returns 400 Bad Request
#[tokio::test]
async fn filter_invalid_expression_returns_400() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("filter-invalid-expr", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Send an invalid CQL2 filter: an opening brace '{' is not valid CQL2 text syntax
    // and will cause a parse error (percent-encoded: %7B is '{')
    let response = app
        .get(&format!(
            "/collections/{}/items?filter=%7Bnot-valid-cql2&filter-lang=cql2-text",
            collection_id
        ))
        .await;

    assert_eq!(
        response.status,
        StatusCode::BAD_REQUEST,
        "Invalid filter expression must return 400 Bad Request"
    );
}

/// A.3.7: filter-crs parameter is accepted for spatial filters
#[tokio::test]
async fn filter_crs_parameter_accepted() {
    let app = TestApp::new().await;

    // Create a collection
    let collection = test_collection_request("filter-crs-test", "vector");
    let create_response = app.post_json("/collections", &collection).await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let collection_id = created["id"].as_str().expect("Collection must have id");

    // Create a feature
    let feature = serde_json::json!({
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "TestPoint", "value": 1 }
    });
    app.post_json(&format!("/collections/{}/items", collection_id), &feature)
        .await
        .assert_status(StatusCode::CREATED);

    // Request with filter-crs parameter (WGS84 default)
    let crs = "http://www.opengis.net/def/crs/OGC/1.3/CRS84";
    let response = app
        .get(&format!(
            "/collections/{}/items?filter-crs={}&filter-lang=cql2-text",
            collection_id, crs
        ))
        .await;

    // Should succeed even without a filter (filter-crs is advisory when no spatial filter)
    response.assert_success();
}

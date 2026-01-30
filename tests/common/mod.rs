//! Common test utilities and fixtures
//!
//! This module provides infrastructure for running integration tests without
//! external dependencies like OIDC providers. Uses testcontainers for the
//! database and mock authentication.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::Response,
    Extension, Router,
};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use std::sync::{Arc, Once};
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    ContainerAsync, GenericImage, ImageExt,
};
use tower::ServiceExt;

use spatialvault::{
    api::{collections, conformance, coverages, features, landing, processes, stac, tiles},
    auth::AuthenticatedUser,
    config::{Config, DatabaseConfig, OidcConfig, S3Config},
    db::Database,
    openapi,
    services::{
        CollectionService, CoverageService, FeatureService, ProcessService, StacService,
        TileService,
    },
};

static INIT: Once = Once::new();

/// Initialize test logging
pub fn init_logging() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter("spatialvault=debug,testcontainers=info")
            .with_test_writer()
            .try_init()
            .ok();
    });
}

/// Assert that a response has a specific link relation
pub fn assert_has_link(links: &[serde_json::Value], rel: &str) -> bool {
    links.iter().any(|link| {
        link.get("rel")
            .and_then(|r| r.as_str())
            .map(|r| r == rel)
            .unwrap_or(false)
    })
}

// ============================================================================
// PostGIS Container
// ============================================================================

/// PostGIS container for integration tests
pub struct PostgisContainer {
    container: ContainerAsync<GenericImage>,
    port: u16,
}

impl PostgisContainer {
    /// Start a new PostGIS container
    ///
    /// # Panics
    /// Panics if Docker is not available or the container fails to start.
    /// To run these tests, ensure Docker is installed and the current user
    /// has permission to access the Docker socket (e.g., user is in docker group).
    pub async fn start() -> Self {
        let container = GenericImage::new("postgis/postgis", "16-3.4")
            .with_exposed_port(5432.tcp())
            .with_wait_for(WaitFor::message_on_stderr("database system is ready to accept connections"))
            .with_env_var("POSTGRES_USER", "postgres")
            .with_env_var("POSTGRES_PASSWORD", "postgres")
            .with_env_var("POSTGRES_DB", "spatialvault_test")
            .start()
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "Failed to start PostGIS container: {:?}\n\n\
                    To run standalone integration tests, ensure:\n\
                    1. Docker is installed and running\n\
                    2. Current user has Docker access (add to 'docker' group or use sudo)\n\
                    3. Network connectivity for pulling images\n\n\
                    Alternatively, run tests against an external server:\n\
                    TEST_BASE_URL=http://localhost:8080 cargo test --test ogc_tests -- --ignored",
                    e
                )
            });

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get container port");

        Self { container, port }
    }

    /// Get the database connection URL
    pub fn connection_url(&self) -> String {
        format!(
            "postgresql://postgres:postgres@127.0.0.1:{}/spatialvault_test",
            self.port
        )
    }
}


// ============================================================================
// Mock Authentication
// ============================================================================

/// State for mock authentication
#[derive(Clone)]
pub struct MockAuthState {
    /// The user to inject for all requests
    pub user: AuthenticatedUser,
}

impl Default for MockAuthState {
    fn default() -> Self {
        Self {
            user: AuthenticatedUser {
                username: "testuser".to_string(),
                subject: "test-subject-id".to_string(),
                groups: vec!["test-group".to_string()],
            },
        }
    }
}

impl MockAuthState {
    /// Create a mock auth state with a specific username
    pub fn with_username(username: impl Into<String>) -> Self {
        Self {
            user: AuthenticatedUser {
                username: username.into(),
                subject: "test-subject-id".to_string(),
                groups: vec![],
            },
        }
    }

    /// Create a mock auth state with groups
    pub fn with_groups(username: impl Into<String>, groups: Vec<String>) -> Self {
        Self {
            user: AuthenticatedUser {
                username: username.into(),
                subject: "test-subject-id".to_string(),
                groups,
            },
        }
    }
}

/// Mock authentication middleware that injects a test user
pub async fn mock_auth_middleware(
    State(auth): State<MockAuthState>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    // Check for Authorization header (optional - but we still inject the user)
    let has_auth = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .map(|h| h.starts_with("Bearer "))
        .unwrap_or(false);

    // For tests, we can optionally require the header
    // For now, we'll inject the user regardless
    if !has_auth {
        // In strict mode, we could reject here
        // For convenience in tests, we'll allow it
    }

    // Inject the test user
    request.extensions_mut().insert(auth.user.clone());

    Ok(next.run(request).await)
}

// ============================================================================
// Test Application Builder
// ============================================================================

/// A test application with an in-process router and database
pub struct TestApp {
    pub router: Router,
    pub db: Arc<Database>,
    pub config: Arc<Config>,
    _container: Option<PostgisContainer>,
}

impl TestApp {
    /// Create a new test application with a PostGIS container
    pub async fn new() -> Self {
        Self::with_auth(MockAuthState::default()).await
    }

    /// Create a new test application with custom mock auth
    pub async fn with_auth(mock_auth: MockAuthState) -> Self {
        init_logging();

        // Start PostGIS container
        let container = PostgisContainer::start().await;

        // Wait a bit for the database to be fully ready
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Create config
        let config = Arc::new(Config {
            host: "127.0.0.1".to_string(),
            port: 0, // Not used for in-process testing
            database: DatabaseConfig {
                url: container.connection_url(),
                max_connections: 5,
                service_role: "postgres".to_string(), // Use postgres for testing
            },
            oidc: OidcConfig {
                issuer_url: "http://localhost".to_string(), // Not used with mock auth
                audience: "test".to_string(),
            },
            s3: S3Config::default(),
            base_url: "http://localhost:8080".to_string(),
        });

        // Connect to database
        let db = Arc::new(
            Database::connect(&config.database)
                .await
                .expect("Failed to connect to database"),
        );

        // Run migrations
        db.run_migrations()
            .await
            .expect("Failed to run migrations");

        // Create services
        let collection_service = Arc::new(CollectionService::new(db.clone()));
        let feature_service = Arc::new(FeatureService::new(db.clone()));
        let tile_service = Arc::new(TileService::new(db.clone()));
        let coverage_service = Arc::new(CoverageService::new(db.clone()));
        let process_service = Arc::new(ProcessService::new(db.clone()));
        let stac_service = Arc::new(StacService::new(db.clone(), config.base_url.clone()));

        // Create OpenAPI spec
        let openapi = Arc::new(openapi::create_openapi(&config));

        // Build router with mock auth
        let router = Self::build_router(
            config.clone(),
            openapi,
            mock_auth,
            collection_service,
            feature_service,
            tile_service,
            coverage_service,
            process_service,
            stac_service,
        );

        Self {
            router,
            db,
            config,
            _container: Some(container),
        }
    }

    /// Build the router with mock authentication
    fn build_router(
        config: Arc<Config>,
        openapi: Arc<aide::openapi::OpenApi>,
        mock_auth: MockAuthState,
        collection_service: Arc<CollectionService>,
        feature_service: Arc<FeatureService>,
        tile_service: Arc<TileService>,
        coverage_service: Arc<CoverageService>,
        process_service: Arc<ProcessService>,
        stac_service: Arc<StacService>,
    ) -> Router {
        use axum::middleware;

        // Public routes (no auth required)
        let public_routes = Router::new()
            .merge(landing::routes())
            .merge(conformance::routes())
            .merge(openapi::docs_routes())
            .merge(stac::catalog::routes());

        // Protected routes (with mock auth)
        let protected_routes = Router::new()
            .merge(collections::handlers::routes(collection_service.clone()))
            .merge(collections::sharing::routes(collection_service.clone()))
            .merge(features::handlers::routes(feature_service, collection_service.clone()))
            .merge(tiles::handlers::routes(tile_service, collection_service.clone()))
            .merge(coverages::handlers::routes(coverage_service, collection_service.clone()))
            .merge(processes::handlers::routes(process_service))
            .merge(stac::item::routes(stac_service))
            .layer(middleware::from_fn_with_state(mock_auth, mock_auth_middleware));

        // Combine and add extensions
        Router::new()
            .merge(public_routes)
            .merge(protected_routes)
            .layer(Extension(config))
            .layer(Extension(openapi))
    }

    /// Make a GET request to the test app
    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(Method::GET, uri, Body::empty()).await
    }

    /// Make a POST request with JSON body
    pub async fn post_json(&self, uri: &str, body: &impl serde::Serialize) -> TestResponse {
        let body = serde_json::to_string(body).expect("Failed to serialize body");
        self.request_with_content_type(Method::POST, uri, body, "application/json")
            .await
    }

    /// Make a PUT request with JSON body
    pub async fn put_json(
        &self,
        uri: &str,
        body: &impl serde::Serialize,
        etag: &str,
    ) -> TestResponse {
        let body = serde_json::to_string(body).expect("Failed to serialize body");
        self.request_with_headers(
            Method::PUT,
            uri,
            body,
            vec![
                (header::CONTENT_TYPE, "application/json"),
                (header::IF_MATCH, etag),
            ],
        )
        .await
    }

    /// Make a PATCH request with JSON body
    pub async fn patch_json(
        &self,
        uri: &str,
        body: &impl serde::Serialize,
        etag: &str,
    ) -> TestResponse {
        let body = serde_json::to_string(body).expect("Failed to serialize body");
        self.request_with_headers(
            Method::PATCH,
            uri,
            body,
            vec![
                (header::CONTENT_TYPE, "application/merge-patch+json"),
                (header::IF_MATCH, etag),
            ],
        )
        .await
    }

    /// Make a DELETE request
    pub async fn delete(&self, uri: &str, etag: &str) -> TestResponse {
        self.request_with_headers(
            Method::DELETE,
            uri,
            String::new(),
            vec![(header::IF_MATCH, etag)],
        )
        .await
    }

    /// Make a request without ETag (for endpoints that don't require it, like job dismiss)
    pub async fn request_without_etag(&self, method: Method, uri: &str) -> TestResponse {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, "Bearer test-token")
            .body(Body::empty())
            .expect("Failed to build request");

        self.send(request).await
    }

    /// Make a PATCH request without If-Match header (to test ETag requirement)
    pub async fn patch_json_without_etag(
        &self,
        uri: &str,
        body: &impl serde::Serialize,
    ) -> TestResponse {
        let body = serde_json::to_string(body).expect("Failed to serialize body");
        let request = Request::builder()
            .method(Method::PATCH)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/merge-patch+json")
            .header(header::AUTHORIZATION, "Bearer test-token")
            .body(Body::from(body))
            .expect("Failed to build request");

        self.send(request).await
    }

    /// Ensure a role exists in the database (for sharing tests)
    pub async fn ensure_role_exists(&self, role_name: &str) {
        sqlx::query("SELECT spatialvault.ensure_role($1)")
            .bind(role_name)
            .execute(self.db.pool())
            .await
            .expect("Failed to ensure role exists");
    }

    /// Make a request with specific headers
    async fn request_with_headers(
        &self,
        method: Method,
        uri: &str,
        body: String,
        headers: Vec<(header::HeaderName, &str)>,
    ) -> TestResponse {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, "Bearer test-token");

        for (name, value) in headers {
            builder = builder.header(name, value);
        }

        let request = builder
            .body(Body::from(body))
            .expect("Failed to build request");

        self.send(request).await
    }

    /// Make a request with content type
    async fn request_with_content_type(
        &self,
        method: Method,
        uri: &str,
        body: String,
        content_type: &str,
    ) -> TestResponse {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::AUTHORIZATION, "Bearer test-token")
            .body(Body::from(body))
            .expect("Failed to build request");

        self.send(request).await
    }

    /// Make a raw request
    async fn request(&self, method: Method, uri: &str, body: Body) -> TestResponse {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, "Bearer test-token")
            .body(body)
            .expect("Failed to build request");

        self.send(request).await
    }

    /// Send a request to the router
    async fn send(&self, request: Request) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("Failed to send request");

        TestResponse::from_response(response).await
    }
}

// ============================================================================
// Test Response
// ============================================================================

/// A test response with convenient methods for assertions
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl TestResponse {
    async fn from_response(response: Response) -> Self {
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to read body")
            .to_bytes()
            .to_vec();

        Self {
            status,
            headers,
            body,
        }
    }

    /// Get the response body as a string
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// Parse the response body as JSON
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).expect("Failed to parse JSON")
    }

    /// Get the ETag header value
    pub fn etag(&self) -> Option<String> {
        self.headers
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    /// Get the Location header value
    pub fn location(&self) -> Option<String> {
        self.headers
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    /// Get a header value
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    /// Assert the status code
    pub fn assert_status(&self, expected: StatusCode) -> &Self {
        assert_eq!(
            self.status, expected,
            "Expected status {}, got {}. Body: {}",
            expected,
            self.status,
            self.text()
        );
        self
    }

    /// Assert the status is success (2xx)
    pub fn assert_success(&self) -> &Self {
        assert!(
            self.status.is_success(),
            "Expected success status, got {}. Body: {}",
            self.status,
            self.text()
        );
        self
    }

    /// Assert content type header
    pub fn assert_content_type(&self, expected: &str) -> &Self {
        let content_type = self.header("content-type").unwrap_or_default();
        assert!(
            content_type.starts_with(expected),
            "Expected content type starting with {}, got {}",
            expected,
            content_type
        );
        self
    }
}

// ============================================================================
// Test Fixtures
// ============================================================================

/// Create a test collection request
pub fn test_collection_request(id: &str, collection_type: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "title": format!("Test Collection {}", id),
        "description": "A test collection",
        "collectionType": collection_type,
        "crs": 4326
    })
}

/// Create a test feature request
pub fn test_feature_request() -> serde_json::Value {
    serde_json::json!({
        "type": "Feature",
        "geometry": {
            "type": "Point",
            "coordinates": [0.0, 0.0]
        },
        "properties": {
            "name": "Test Feature",
            "value": 42
        }
    })
}

/// Create a test STAC item request
pub fn test_stac_item_request() -> serde_json::Value {
    serde_json::json!({
        "type": "Feature",
        "geometry": {
            "type": "Polygon",
            "coordinates": [[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]]
        },
        "properties": {
            "datetime": "2024-01-15T12:00:00Z",
            "title": "Test STAC Item"
        },
        "assets": {
            "data": {
                "href": "s3://bucket/test.tif",
                "type": "image/tiff; application=geotiff; profile=cloud-optimized",
                "roles": ["data"]
            }
        }
    })
}

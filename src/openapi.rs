use aide::{
    axum::{ApiRouter, routing::get_with},
    openapi::{
        Components, Contact, ExternalDocumentation, Info, License, OpenApi, ReferenceOr, Server,
        Tag,
    },
    transform::TransformOperation,
};
use axum::{Extension, Json};
use indexmap::IndexMap;
use schemars::schema_for;
use std::sync::Arc;

use crate::api::collections::schemas::{
    CollectionResponse, CollectionsResponse, CreateCollectionRequest, UpdateCollectionRequest,
};
use crate::api::common::{Extent, Link};
use crate::api::features::handlers::{Feature, FeatureCollection};
use crate::config::Config;

/// Create the base OpenAPI specification with metadata
pub fn create_openapi(config: &Config) -> OpenApi {
    // Build components with schemas from our types
    let mut schemas = IndexMap::new();

    schemas.insert("Link".to_string(), schemars_to_openapi_schema::<Link>());
    schemas.insert("Extent".to_string(), schemars_to_openapi_schema::<Extent>());
    schemas.insert(
        "CollectionResponse".to_string(),
        schemars_to_openapi_schema::<CollectionResponse>(),
    );
    schemas.insert(
        "CollectionsResponse".to_string(),
        schemars_to_openapi_schema::<CollectionsResponse>(),
    );
    schemas.insert(
        "CreateCollectionRequest".to_string(),
        schemars_to_openapi_schema::<CreateCollectionRequest>(),
    );
    schemas.insert(
        "UpdateCollectionRequest".to_string(),
        schemars_to_openapi_schema::<UpdateCollectionRequest>(),
    );
    schemas.insert(
        "Feature".to_string(),
        schemars_to_openapi_schema::<Feature>(),
    );
    schemas.insert(
        "FeatureCollection".to_string(),
        schemars_to_openapi_schema::<FeatureCollection>(),
    );

    let mut security_schemes = IndexMap::new();
    security_schemes.insert(
        "bearerAuth".to_string(),
        ReferenceOr::Item(aide::openapi::SecurityScheme::Http {
            scheme: "bearer".to_string(),
            bearer_format: Some("JWT".to_string()),
            description: Some("JWT token from OIDC provider".to_string()),
            extensions: IndexMap::new(),
        }),
    );

    let components = Components {
        schemas,
        security_schemes,
        ..Default::default()
    };

    OpenApi {
        openapi: "3.0.3".into(),
        info: Info {
            title: "SpatialVault API".to_string(),
            description: Some(
                "OGC API compliant geospatial data service with STAC integration.\n\n\
                ## Supported Standards\n\n\
                - **OGC API - Features** (Part 1: Core, Part 4: CRS)\n\
                - **OGC API - Tiles** (Core)\n\
                - **OGC API - Coverages** (Core)\n\
                - **OGC API - Processes** (Core)\n\
                - **STAC API** (Core, Transaction extensions)\n\n\
                ## Authentication\n\n\
                All protected endpoints require a Bearer token from the configured OIDC provider.\n\n\
                ## Optimistic Locking\n\n\
                All modification operations (PUT, PATCH, DELETE) require an `If-Match` header \
                containing the current ETag of the resource."
                    .to_string(),
            ),
            version: env!("CARGO_PKG_VERSION").to_string(),
            contact: Some(Contact {
                name: Some("SpatialVault".to_string()),
                url: Some("https://github.com/spatialvault".to_string()),
                email: None,
                extensions: IndexMap::new(),
            }),
            license: Some(License {
                name: "MIT".to_string(),
                url: Some("https://opensource.org/licenses/MIT".to_string()),
                identifier: None,
                extensions: IndexMap::new(),
            }),
            terms_of_service: None,
            summary: None,
            extensions: IndexMap::new(),
        },
        servers: vec![Server {
            url: config.base_url.clone(),
            description: Some("SpatialVault Server".to_string()),
            variables: IndexMap::new(),
            extensions: IndexMap::new(),
        }],
        components: Some(components),
        tags: vec![
            Tag {
                name: "Core".to_string(),
                description: Some(
                    "Core OGC API endpoints (landing page, conformance, API definition)".to_string(),
                ),
                external_docs: None,
                extensions: IndexMap::new(),
            },
            Tag {
                name: "Collections".to_string(),
                description: Some("Collection management operations".to_string()),
                external_docs: Some(ExternalDocumentation {
                    url: "https://docs.ogc.org/is/17-069r4/17-069r4.html".to_string(),
                    description: Some("OGC API - Features specification".to_string()),
                    extensions: IndexMap::new(),
                }),
                extensions: IndexMap::new(),
            },
            Tag {
                name: "Features".to_string(),
                description: Some("Feature/item CRUD operations".to_string()),
                external_docs: Some(ExternalDocumentation {
                    url: "https://docs.ogc.org/is/17-069r4/17-069r4.html".to_string(),
                    description: Some("OGC API - Features specification".to_string()),
                    extensions: IndexMap::new(),
                }),
                extensions: IndexMap::new(),
            },
            Tag {
                name: "Tiles".to_string(),
                description: Some("Tile access for vector and raster data".to_string()),
                external_docs: Some(ExternalDocumentation {
                    url: "https://docs.ogc.org/is/20-057/20-057.html".to_string(),
                    description: Some("OGC API - Tiles specification".to_string()),
                    extensions: IndexMap::new(),
                }),
                extensions: IndexMap::new(),
            },
            Tag {
                name: "Coverages".to_string(),
                description: Some("Coverage data access for raster collections".to_string()),
                external_docs: Some(ExternalDocumentation {
                    url: "https://docs.ogc.org/is/19-087r1/19-087r1.html".to_string(),
                    description: Some("OGC API - Coverages specification".to_string()),
                    extensions: IndexMap::new(),
                }),
                extensions: IndexMap::new(),
            },
            Tag {
                name: "Processes".to_string(),
                description: Some(
                    "Async processing jobs (import-raster, import-pointcloud)".to_string(),
                ),
                external_docs: Some(ExternalDocumentation {
                    url: "https://docs.ogc.org/is/18-062r2/18-062r2.html".to_string(),
                    description: Some("OGC API - Processes specification".to_string()),
                    extensions: IndexMap::new(),
                }),
                extensions: IndexMap::new(),
            },
            Tag {
                name: "STAC".to_string(),
                description: Some("SpatioTemporal Asset Catalog endpoints".to_string()),
                external_docs: Some(ExternalDocumentation {
                    url: "https://stacspec.org/".to_string(),
                    description: Some("STAC Specification".to_string()),
                    extensions: IndexMap::new(),
                }),
                extensions: IndexMap::new(),
            },
            Tag {
                name: "Sharing".to_string(),
                description: Some("Collection sharing and permissions".to_string()),
                external_docs: None,
                extensions: IndexMap::new(),
            },
        ],
        paths: None, // Will be populated by ApiRouter
        webhooks: IndexMap::new(),
        external_docs: None,
        extensions: IndexMap::new(),
        json_schema_dialect: None,
        security: vec![],
    }
}

/// Convert a schemars schema to an aide SchemaObject
fn schemars_to_openapi_schema<T: schemars::JsonSchema>() -> aide::openapi::SchemaObject {
    let root = schema_for!(T);
    aide::openapi::SchemaObject {
        json_schema: root.into(),
        external_docs: None,
        example: None,
    }
}

/// Handler to serve the OpenAPI specification
pub async fn openapi_handler(Extension(api): Extension<Arc<OpenApi>>) -> Json<OpenApi> {
    Json((*api).clone())
}

fn openapi_handler_docs(op: TransformOperation) -> TransformOperation {
    op.summary("OpenAPI specification")
        .description("Returns the OpenAPI 3.0 specification for this API")
        .tag("Core")
}

/// Create the docs route that serves the OpenAPI spec
pub fn docs_routes() -> ApiRouter {
    ApiRouter::new().api_route("/api", get_with(openapi_handler, openapi_handler_docs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{conformance, landing, stac};

    fn test_config() -> Config {
        Config {
            host: "127.0.0.1".to_string(),
            port: 8080,
            database: crate::config::DatabaseConfig {
                url: "postgres://localhost/test".to_string(),
                max_connections: 5,
                service_role: "test".to_string(),
            },
            oidc: crate::config::OidcConfig {
                issuer_url: "http://localhost".to_string(),
                audience: "test".to_string(),
            },
            s3: crate::config::S3Config::default(),
            base_url: "http://localhost:8080".to_string(),
        }
    }

    #[test]
    fn test_openapi_paths_populated() {
        let config = test_config();
        let mut openapi = create_openapi(&config);

        // Initially, paths should be None
        assert!(
            openapi.paths.is_none(),
            "Paths should be None before finish_api"
        );

        // Build a minimal router with some routes
        let _router = ApiRouter::new()
            .merge(landing::routes())
            .merge(conformance::routes())
            .merge(docs_routes())
            .finish_api(&mut openapi);

        // After finish_api, paths should be populated
        assert!(
            openapi.paths.is_some(),
            "Paths should be Some after finish_api"
        );

        let paths = openapi.paths.as_ref().unwrap();

        // Check that our routes are registered
        assert!(
            paths.paths.contains_key("/"),
            "Landing page route should be registered"
        );
        assert!(
            paths.paths.contains_key("/conformance"),
            "Conformance route should be registered"
        );
        assert!(
            paths.paths.contains_key("/api"),
            "API docs route should be registered"
        );

        // Verify the paths count (should have at least 3 paths)
        assert!(
            paths.paths.len() >= 3,
            "Should have at least 3 paths, got {}",
            paths.paths.len()
        );
    }

    #[test]
    fn test_openapi_includes_stac_routes() {
        let config = test_config();
        let mut openapi = create_openapi(&config);

        let _router = ApiRouter::new()
            .merge(stac::catalog::routes())
            .finish_api(&mut openapi);

        let paths = openapi.paths.as_ref().unwrap();

        assert!(
            paths.paths.contains_key("/stac"),
            "STAC catalog route should be registered"
        );
    }

    #[test]
    fn test_openapi_spec_has_info() {
        let config = test_config();
        let openapi = create_openapi(&config);

        assert_eq!(openapi.info.title, "SpatialVault API");
        assert!(!openapi.info.version.is_empty());
        assert!(openapi.info.description.is_some());
    }

    #[test]
    fn test_openapi_spec_has_tags() {
        let config = test_config();
        let openapi = create_openapi(&config);

        let tag_names: Vec<&str> = openapi.tags.iter().map(|t| t.name.as_str()).collect();

        assert!(tag_names.contains(&"Core"));
        assert!(tag_names.contains(&"Collections"));
        assert!(tag_names.contains(&"Features"));
        assert!(tag_names.contains(&"Tiles"));
        assert!(tag_names.contains(&"Coverages"));
        assert!(tag_names.contains(&"Processes"));
        assert!(tag_names.contains(&"STAC"));
    }

    #[test]
    fn test_openapi_spec_has_security_scheme() {
        let config = test_config();
        let openapi = create_openapi(&config);

        let components = openapi.components.as_ref().unwrap();
        assert!(
            components.security_schemes.contains_key("bearerAuth"),
            "Should have bearerAuth security scheme"
        );
    }
}

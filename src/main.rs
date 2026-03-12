use aide::axum::ApiRouter;
use axum::http::{Method, header};
use axum::{Extension, Router, middleware};
use std::env;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use spatialvault::{
    api::{collections, conformance, coverages, features, landing, processes, stac, tiles},
    auth::{self, AuthState, OidcValidator},
    config::Config,
    db::Database,
    openapi,
    processing::JobWorker,
    services::{
        CollectionService, CoverageService, FeatureService, ItemService, ProcessService,
        StacService, TileService,
    },
    storage::S3Storage,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "spatialvault=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Check for worker mode
    let args: Vec<String> = env::args().collect();
    let worker_mode = args.iter().any(|arg| arg == "--worker" || arg == "-w");

    // Load configuration
    let config = Config::load()?;

    // Connect to database
    let db = Arc::new(Database::connect(&config.database).await?);
    tracing::info!("Connected to database");

    // Run migrations
    db.run_migrations().await?;
    tracing::info!("Migrations complete");

    // Initialize S3 storage (optional)
    let storage = match config.s3 {
        Some(ref s3_config) => {
            let s = Arc::new(S3Storage::new(s3_config)?);
            tracing::info!("S3 storage initialized");
            Some(s)
        }
        None => {
            tracing::info!(
                "No S3 storage configured — file uploads will be imported synchronously"
            );
            None
        }
    };

    // Create services
    let collection_service = Arc::new(CollectionService::new(db.clone(), config.base_url.clone()));
    let feature_service = Arc::new(FeatureService::new(db.clone(), collection_service.clone()));
    let tile_service = Arc::new(TileService::new(db.clone()));
    let coverage_service = Arc::new(CoverageService::new(db.clone(), collection_service.clone()));
    let process_service = Arc::new(ProcessService::new(db.clone()));
    let stac_service = Arc::new(StacService::new(
        db.clone(),
        config.base_url.clone(),
        collection_service.clone(),
        feature_service.clone(),
    ));
    let item_service = Arc::new(ItemService::new(db.clone()));

    if worker_mode {
        // Run as background job worker — S3 is required
        let storage = storage.ok_or_else(|| {
            anyhow::anyhow!(
                "S3 storage configuration is required for worker mode. \
                 Set SPATIALVAULT__S3__BUCKET and related environment variables."
            )
        })?;
        tracing::info!("Starting SpatialVault in worker mode");

        let worker = JobWorker::new(
            db.clone(),
            storage,
            process_service,
            item_service,
            collection_service,
        );

        worker.run().await?;
    } else {
        // Run as HTTP server
        tracing::info!("Starting SpatialVault on {}:{}", config.host, config.port);

        // Build router with OpenAPI generation
        let app = if config.auth.disabled {
            tracing::warn!("Authentication is DISABLED — all requests will use an anonymous user");
            build_router_no_auth(
                config.clone(),
                storage,
                collection_service,
                feature_service,
                tile_service,
                coverage_service,
                process_service,
                stac_service,
            )
        } else {
            // Initialize OIDC validator
            let oidc_config = config.oidc.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "OIDC configuration is required when auth is not disabled. \
                     Set SPATIALVAULT__OIDC__ISSUER_URL or set SPATIALVAULT__AUTH__DISABLED=true"
                )
            })?;
            let oidc_validator = Arc::new(OidcValidator::new(oidc_config).await?);
            tracing::info!("OIDC validator initialized");

            if config.auth.dev_auth {
                tracing::warn!(
                    "Dev auth is ENABLED — 'Authorization: User <name>' headers will bypass OIDC"
                );
            }

            let auth_state = AuthState {
                validator: oidc_validator,
                dev_auth: config.auth.dev_auth,
            };

            build_router(
                config.clone(),
                auth_state,
                storage,
                collection_service,
                feature_service,
                tile_service,
                coverage_service,
                process_service,
                stac_service,
            )
        };

        // Optionally nest under a path prefix derived from base_url
        let app = match config.path_prefix().as_deref() {
            Some(p) => {
                tracing::info!("Mounting API under path prefix: {}", p);
                Router::new().nest(p, app)
            }
            None => app,
        };

        // Start server
        let addr = format!("{}:{}", config.host, config.port);
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("Listening on {}", addr);

        axum::serve(listener, app).await?;
    }

    Ok(())
}

fn build_router(
    config: Arc<Config>,
    auth_state: AuthState,
    storage: Option<Arc<S3Storage>>,
    collection_service: Arc<CollectionService>,
    feature_service: Arc<FeatureService>,
    tile_service: Arc<TileService>,
    coverage_service: Arc<CoverageService>,
    process_service: Arc<ProcessService>,
    stac_service: Arc<StacService>,
) -> Router {
    // Create base OpenAPI spec with metadata
    let mut openapi = openapi::create_openapi(&config);

    // Public routes (optional auth — landing page uses it for currentUser)
    let public_routes = ApiRouter::new()
        .merge(landing::routes())
        .merge(conformance::routes())
        .merge(openapi::docs_routes())
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            spatialvault::auth::optional_auth_middleware,
        ));

    // Protected routes (auth required)
    let protected_routes = ApiRouter::new()
        .merge(collections::handlers::routes(
            storage.clone(),
            collection_service.clone(),
            process_service.clone(),
        ))
        .merge(collections::sharing::routes(collection_service.clone()))
        .merge(features::handlers::routes(storage, feature_service))
        .merge(tiles::handlers::routes(
            tile_service,
            collection_service.clone(),
        ))
        .merge(coverages::handlers::routes(
            coverage_service,
            collection_service.clone(),
        ))
        .merge(processes::handlers::routes(process_service))
        .merge(stac::item::routes(stac_service))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            spatialvault::auth::auth_middleware,
        ));

    // Combine all routes and generate OpenAPI spec
    let api_router = ApiRouter::new()
        .merge(public_routes)
        .merge(protected_routes)
        .finish_api(&mut openapi);

    // Wrap OpenAPI in Arc for sharing
    let openapi = Arc::new(openapi);

    // Convert to regular Router and add extensions/layers
    let cors = build_cors_layer(&config);
    Router::from(api_router)
        .layer(Extension(config))
        .layer(Extension(openapi))
        .layer(middleware::from_fn(
            spatialvault::api::link_header_middleware,
        ))
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

fn build_router_no_auth(
    config: Arc<Config>,
    storage: Option<Arc<S3Storage>>,
    collection_service: Arc<CollectionService>,
    feature_service: Arc<FeatureService>,
    tile_service: Arc<TileService>,
    coverage_service: Arc<CoverageService>,
    process_service: Arc<ProcessService>,
    stac_service: Arc<StacService>,
) -> Router {
    let mut openapi = openapi::create_openapi(&config);

    let public_routes = ApiRouter::new()
        .merge(landing::routes())
        .merge(conformance::routes())
        .merge(openapi::docs_routes())
        .layer(middleware::from_fn(auth::no_auth_middleware));

    let protected_routes = ApiRouter::new()
        .merge(collections::handlers::routes(
            storage.clone(),
            collection_service.clone(),
            process_service.clone(),
        ))
        .merge(collections::sharing::routes(collection_service.clone()))
        .merge(features::handlers::routes(storage, feature_service))
        .merge(tiles::handlers::routes(
            tile_service,
            collection_service.clone(),
        ))
        .merge(coverages::handlers::routes(
            coverage_service,
            collection_service.clone(),
        ))
        .merge(processes::handlers::routes(process_service))
        .merge(stac::item::routes(stac_service))
        .layer(middleware::from_fn(auth::no_auth_middleware));

    let api_router = ApiRouter::new()
        .merge(public_routes)
        .merge(protected_routes)
        .finish_api(&mut openapi);

    let openapi = Arc::new(openapi);

    let cors = build_cors_layer(&config);
    Router::from(api_router)
        .layer(Extension(config))
        .layer(Extension(openapi))
        .layer(middleware::from_fn(
            spatialvault::api::link_header_middleware,
        ))
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

fn build_cors_layer(config: &Config) -> CorsLayer {
    let cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::HEAD,
            Method::OPTIONS,
            Method::PUT,
            Method::PATCH,
            Method::POST,
            Method::DELETE,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    match &config.cors_origins {
        Some(pattern) => {
            let re = regex::Regex::new(&format!("^(?:{pattern})$"))
                .expect("Invalid cors_origins regex pattern");
            tracing::info!("CORS: allowing origins matching {}", re.as_str());
            cors.allow_origin(AllowOrigin::predicate(move |origin, _| {
                origin.as_bytes().iter().all(|b| b.is_ascii())
                    && re.is_match(origin.to_str().unwrap_or(""))
            }))
        }
        None => {
            tracing::info!("CORS: allowing all origins");
            cors.allow_origin(Any)
        }
    }
}

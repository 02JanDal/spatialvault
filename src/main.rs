use aide::axum::ApiRouter;
use axum::{middleware, Extension, Router};
use std::env;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use spatialvault::{
    api::{collections, conformance, coverages, features, landing, processes, stac, tiles},
    auth::{AuthState, OidcValidator},
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
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "spatialvault=debug,tower_http=debug,axum::rejection=trace".into()
        }))
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

    // Initialize S3 storage
    let storage = Arc::new(S3Storage::new(&config.s3)?);
    tracing::info!("S3 storage initialized");

    // Create services
    let collection_service = Arc::new(CollectionService::new(db.clone()));
    let feature_service = Arc::new(FeatureService::new(db.clone()));
    let tile_service = Arc::new(TileService::new(db.clone()));
    let coverage_service = Arc::new(CoverageService::new(db.clone()));
    let process_service = Arc::new(ProcessService::new(db.clone()));
    let stac_service = Arc::new(StacService::new(db.clone(), config.base_url.clone()));
    let item_service = Arc::new(ItemService::new(db.clone()));

    if worker_mode {
        // Run as background job worker
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

        // Initialize OIDC validator
        let oidc_validator = Arc::new(OidcValidator::new(config.oidc.clone()).await?);
        tracing::info!("OIDC validator initialized");

        // Build auth state
        let auth_state = AuthState {
            validator: oidc_validator,
        };

        // Build router with OpenAPI generation
        let app = build_router(
            config.clone(),
            auth_state,
            collection_service,
            feature_service,
            tile_service,
            coverage_service,
            process_service,
            stac_service,
        );

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
    collection_service: Arc<CollectionService>,
    feature_service: Arc<FeatureService>,
    tile_service: Arc<TileService>,
    coverage_service: Arc<CoverageService>,
    process_service: Arc<ProcessService>,
    stac_service: Arc<StacService>,
) -> Router {
    // Create base OpenAPI spec with metadata
    let mut openapi = openapi::create_openapi(&config);

    // Public routes (no auth required)
    let public_routes = ApiRouter::new()
        .merge(landing::routes())
        .merge(conformance::routes())
        .merge(openapi::docs_routes())
        .merge(stac::catalog::routes());

    // Protected routes (auth required)
    let protected_routes = ApiRouter::new()
        .merge(collections::handlers::routes(collection_service.clone()))
        .merge(collections::sharing::routes(collection_service.clone()))
        .merge(features::handlers::routes(feature_service))
        .merge(tiles::handlers::routes(tile_service))
        .merge(coverages::handlers::routes(coverage_service))
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
    Router::from(api_router)
        .layer(Extension(config))
        .layer(Extension(openapi))
        .layer(CompressionLayer::new())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
}

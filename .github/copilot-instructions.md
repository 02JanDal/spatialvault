# Copilot Instructions for SpatialVault

## Project Overview

SpatialVault is a geospatial data management API server built with Rust. It provides OGC-compliant APIs for managing geospatial features, coverages, tiles, and STAC collections with PostgreSQL/PostGIS storage and S3 object storage integration.

### Architecture Clarifications

- **OGC API Processes** manages async jobs (file conversion, processing tasks, etc.)
- **OGC API Coverages** is for raster data only (stored in S3 as COG files)
- **Point clouds** are served via STAC/3D GeoVolumes (stored in S3 as COPC files)
- **CRS** is derived from PostGIS geometry column type (not stored separately)
- **Computed metadata** (bbox, temporal extent) are not stored in the collections table
- **Collections** can be owned by users OR groups
- **Vector data** is stored in user-specific PostgreSQL schemas
- **Raster/point cloud files** are stored in S3-compatible object storage

## Tech Stack

- **Language**: Rust 1.93+ (edition 2024)
- **Web Framework**: Axum 0.8 with Aide 0.15 for OpenAPI generation
- **Database**: PostgreSQL with PostGIS (via SQLx 0.8)
- **Storage**: S3-compatible object storage (via object_store 0.11)
- **Async Runtime**: Tokio 1.x
- **Geospatial**: geo 0.31, geozero 0.14, optional GDAL bindings
- **Auth**: OpenID Connect (OIDC) with JWT validation
- **Testing**: Integration tests with testcontainers

## Build and Test Commands

```bash
# Build the project
cargo build

# Build with all features
cargo build --all-features

# Run tests (requires Docker for integration tests)
cargo test

# Run specific test
cargo test <test_name>

# Check code without building
cargo check

# Format code
cargo fmt

# Run linter
cargo clippy

# Run in worker mode
cargo run -- --worker
```

## Code Style and Best Practices

### General Guidelines

- Follow standard Rust conventions and idioms
- Use `rustfmt` for code formatting (already configured in project)
- Use `clippy` for linting and catching common mistakes
- Prefer explicit error handling with `Result` and `?` operator
- Use `anyhow::Result` for application errors, `thiserror` for library errors
- Keep functions focused and small
- Write descriptive commit messages

### Async/Await

- All async functions use `async/await` with Tokio runtime
- Database operations are async (SQLx)
- HTTP handlers use Axum's async handler pattern
- Use `Arc` for shared state across async tasks

### Error Handling

- Use the project's custom error types in `src/error.rs`
- Propagate errors with `?` operator when possible
- Provide meaningful error messages
- Use `anyhow::Context` to add context to errors

### Database

- Use SQLx for database operations with compile-time query verification
- Migrations are in the `migrations/` directory
- Use prepared statements and parameterized queries
- Handle NULL values explicitly
- Use PostGIS functions for geospatial operations
- Vector data is stored in user-specific PostgreSQL schemas
- CRS is derived from PostGIS geometry column type (e.g., `geometry(Point, 4326)`)
- Computed metadata (bbox, temporal extent) should be calculated at runtime, not stored in collections table

### API Design

- Follow OGC API standards (Features, Tiles, Coverages, Processes)
- Use Aide macros for OpenAPI documentation
- Implement proper pagination for list endpoints
- Return appropriate HTTP status codes
- Use typed routing with `axum-extra`
- **OGC API Features**: Vector data from PostGIS
- **OGC API Tiles**: Both vector (MVT from PostGIS) and raster tiles
- **OGC API Coverages**: Raster data from S3 (COG files)
- **OGC API Processes**: Async job management (conversions, processing)
- **STAC API**: Metadata for raster and point cloud items (COPC files)

### Testing

- Write integration tests in the `tests/` directory
- Use testcontainers for database tests
- Test both success and error cases
- Keep tests focused and independent
- Use descriptive test names

## Project Structure

```
src/
├── api/           - API endpoints and handlers
│   ├── collections/  - OGC API Collections
│   ├── features/     - OGC API Features (vector data)
│   ├── tiles/        - OGC API Tiles (vector & raster)
│   ├── coverages/    - OGC API Coverages (raster data)
│   ├── processes/    - OGC API Processes (async jobs)
│   └── stac/         - STAC API (raster & point cloud metadata)
├── auth/          - OAuth2/OIDC authentication and authorization
├── config.rs      - Configuration management
├── db/            - Database layer (PostGIS vector data, metadata tables)
├── error.rs       - Error types
├── lib.rs         - Library entry point
├── main.rs        - Application entry point (Axum server)
├── openapi.rs     - OpenAPI documentation setup
├── processing/    - Background job processing (worker mode)
├── services/      - Business logic (Features, Tiles, Coverages, STAC, Jobs)
└── storage/       - S3 object storage (COG raster, COPC point cloud files)
tests/
├── common/        - Test utilities
├── integration/   - Integration tests
└── ogc_abstract_tests/ - OGC compliance tests
migrations/        - Database migrations
```

## Boundaries and Constraints

### DO:
- Make minimal, focused changes
- Write tests for new functionality
- Update OpenAPI documentation when changing APIs
- Follow existing patterns in the codebase
- Handle errors gracefully
- Use existing abstractions and services

### DO NOT:
- Commit secrets, credentials, or sensitive data
- Modify database migration files that have already been applied
- Break existing API contracts without versioning
- Remove or disable existing tests
- Introduce unsafe code without justification
- Add dependencies without considering security and maintenance
- Make changes to `.github/` configuration without explicit request
- Modify Docker or deployment configuration without explicit request

## Security Considerations

- All API endpoints should validate input
- Use parameterized queries to prevent SQL injection
- Validate JWT tokens for protected endpoints
- Sanitize user-provided geospatial data
- Handle CORS appropriately
- Never log sensitive information
- Follow the principle of least privilege

## Dependencies

- Check security advisories before adding new dependencies
- Prefer well-maintained crates with good documentation
- Keep dependencies up to date
- Use feature flags to make heavy dependencies optional (like GDAL)

## Common Tasks

### Adding a new API endpoint:
1. Define the handler in `src/api/`
2. Add route in the appropriate module
3. Add OpenAPI documentation with Aide macros
4. Implement service layer logic in `src/services/`
5. Add database queries if needed in `src/db/`
6. Write integration tests

### Adding a new service:
1. Create service struct in `src/services/`
2. Implement business logic
3. Add to service initialization in `main.rs`
4. Write unit and integration tests

### Adding a database migration:
1. Create new migration file in `migrations/`
2. Use standard SQL (compatible with PostgreSQL/PostGIS)
3. Test migration both up and down
4. Never modify existing migration files

## Additional Notes

- The project supports both server and worker modes (background job processing)
  - Server mode: Runs the Axum HTTP server with all API endpoints
  - Worker mode: Runs background job processor for OGC API Processes tasks
- GDAL support is optional via the `gdal-support` feature flag
- The API supports multiple OGC standards: Features, Tiles, Coverages, Processes, STAC
- Authentication is handled via OIDC with JWT validation
- All geospatial data uses WGS84 (EPSG:4326) or Web Mercator (EPSG:3857) coordinate systems
- Collections can be owned by individual users or groups
- File formats:
  - Raster data: Cloud-Optimized GeoTIFF (COG) in S3
  - Point clouds: Cloud-Optimized Point Cloud (COPC) in S3
  - Vector data: PostGIS geometry types in user schemas

//! OGC API Abstract Test Suite
//!
//! This test binary runs OGC API conformance tests. Tests can run in two modes:
//!
//! 1. **Standalone mode** (default): Tests use testcontainers for PostGIS and mock
//!    authentication. No external services required.
//!
//! 2. **External mode** (`--ignored`): Tests connect to an external server specified
//!    by `TEST_BASE_URL` environment variable.
//!
//! # Running Tests
//!
//! ```bash
//! # Run standalone tests (spins up PostGIS container)
//! cargo test --test ogc_tests
//!
//! # Run external tests against a running server
//! TEST_BASE_URL=http://localhost:8080 cargo test --test ogc_tests -- --ignored
//! ```

mod common;
mod ogc_abstract_tests;

// Re-export common utilities for use in test modules
pub use common::*;

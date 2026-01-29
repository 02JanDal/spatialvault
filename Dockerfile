# syntax=docker/dockerfile:1
#
# Multi-stage Dockerfile for SpatialVault
# 
# This Dockerfile creates a production-ready container image following best practices:
# - Multi-stage build to minimize final image size
# - Distroless base image for security (no shell, minimal attack surface)
# - Non-root user for runtime security
# - Optimized layer caching for faster builds
#
# Build: docker build -t spatialvault .
# Run: docker run -p 8080:8080 spatialvault
#
# Note: The application requires configuration for database and S3 storage.
# Configure via environment variables or a config file mounted into the container.

# Build stage - use Rust official image
FROM rust:slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Update CA certificates
RUN update-ca-certificates

# Create application directory
WORKDIR /app

# Copy dependency manifests first for better layer caching
# This allows Docker to cache dependencies separately from source code
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY migrations ./migrations

# Build the application in release mode
RUN cargo build --release

# Runtime stage - use distroless for minimal, secure image
FROM gcr.io/distroless/cc-debian12:nonroot

# Copy the compiled binary from builder stage
COPY --from=builder /app/target/release/spatialvault /usr/local/bin/spatialvault

# Use nonroot user for security
USER nonroot:nonroot

# Expose default application port
# Note: This can be overridden via application configuration
EXPOSE 8080

# Set the entrypoint to the application binary
ENTRYPOINT ["/usr/local/bin/spatialvault"]


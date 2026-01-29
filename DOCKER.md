# Docker Build Guide

## Overview

This repository includes a production-quality Dockerfile that builds a minimal, secure container image for SpatialVault.

## Features

- **Multi-stage build**: Separates build and runtime environments for minimal image size
- **Distroless base**: Uses Google's distroless image for enhanced security (no shell, minimal attack surface)
- **Small image size**: Final image is ~47MB
- **Non-root user**: Runs as non-root for security
- **Layer caching**: Optimized to cache dependencies separately from source code

## Building the Image

```bash
docker build -t spatialvault .
```

## Running the Container

The application requires configuration for database and S3 storage. You can provide configuration via:

### Environment Variables

```bash
docker run -p 8080:8080 \
  -e DATABASE__URL="postgresql://user:pass@host/db" \
  -e S3__BUCKET="my-bucket" \
  -e S3__REGION="us-east-1" \
  spatialvault
```

### Configuration File

```bash
docker run -p 8080:8080 \
  -v /path/to/config:/config \
  -e CONFIG_PATH=/config/config.toml \
  spatialvault
```

## Worker Mode

To run the container in worker mode for background job processing:

```bash
docker run spatialvault --worker
```

## Image Details

- **Base Image**: `gcr.io/distroless/cc-debian12:nonroot`
- **User**: `nonroot:nonroot` (UID 65532)
- **Exposed Port**: 8080
- **Binary Location**: `/usr/local/bin/spatialvault`

## Best Practices Implemented

1. **Multi-stage builds**: Reduces final image size by excluding build tools and dependencies
2. **Distroless base**: Minimizes attack surface by removing unnecessary binaries and libraries
3. **Non-root user**: Improves security by not running as root
4. **Layer optimization**: Copies dependency manifests before source code for better caching
5. **Minimal dependencies**: Only includes runtime dependencies in final image

## Troubleshooting

### SSL Certificate Issues

If you encounter SSL certificate errors during build in corporate environments:

1. Export your CA certificates to a file
2. Copy them to the build context
3. Modify the Dockerfile to include them before the build step

### Build Performance

The first build may take several minutes as it downloads and compiles all Rust dependencies. Subsequent builds will be faster thanks to Docker's layer caching.

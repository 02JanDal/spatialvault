FROM rust:slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    libgdal-dev \
    clang \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

# Build dependencies in a separate layer for caching
# This creates a dummy main.rs to allow cargo to compile dependencies
# The dependencies will be cached and reused when only source code changes
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

COPY src ./src
COPY migrations ./migrations

RUN cargo build --release

# Intermediate stage: install GDAL runtime libs and collect all transitive .so deps
FROM debian:bookworm-slim AS gdal-libs

RUN apt-get update && apt-get install -y \
    libgdal32 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/spatialvault /usr/local/bin/spatialvault

RUN ldd /usr/local/bin/spatialvault \
    | awk '/=>/ { print $3 }' \
    | grep -v '^$' \
    | while read lib; do \
        [ -f "$lib" ] && install -D "$lib" "/runtime-libs/$lib"; \
    done

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=gdal-libs /runtime-libs/ /
COPY --from=gdal-libs /usr/local/bin/spatialvault /usr/local/bin/spatialvault

USER nonroot:nonroot

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/spatialvault"]

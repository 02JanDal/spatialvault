FROM rust:slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
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

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /app/target/release/spatialvault /usr/local/bin/spatialvault

USER nonroot:nonroot

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/spatialvault"]


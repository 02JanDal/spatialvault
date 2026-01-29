FROM rust:slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY .ca-certificate[s].crt* /tmp/
RUN if [ -f /tmp/.ca-certificates.crt ]; then \
        cp /tmp/.ca-certificates.crt /usr/local/share/ca-certificates/custom-ca.crt; \
    fi && \
    update-ca-certificates

COPY Cargo.toml Cargo.lock ./

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


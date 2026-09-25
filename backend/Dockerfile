# Build context is the repository root (see docker-compose.yml) so the default
# policy in policy/ is available to include_str! at compile time.

FROM lukemathwalker/cargo-chef:latest-rust-1.98.1-slim-trixie AS chef
WORKDIR /app/backend

# Dependency recipe: changes only when Cargo.toml/Cargo.lock change.
FROM chef AS planner
COPY backend/ /app/backend/
RUN cargo chef prepare --recipe-path recipe.json

# Build dependencies (cached layer), then the application.
FROM chef AS builder
COPY --from=planner /app/backend/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY backend/ /app/backend/
COPY policy/ /app/policy/
# Compile-time SQL checks use the committed .sqlx metadata; no database needed.
ENV SQLX_OFFLINE=true
RUN cargo build --release --bin refund-api

FROM debian:trixie-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 app
COPY --from=builder /app/backend/target/release/refund-api /usr/local/bin/refund-api
USER app
EXPOSE 8080
ENV BIND_ADDR=0.0.0.0:8080
CMD ["refund-api"]

# syntax=docker/dockerfile:1

# ── Stage 1: Build frontend (WASM) ────────────────────────────────────
FROM rust:1.94-bookworm AS frontend-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev cmake g++ make perl \
    && rm -rf /var/lib/apt/lists/*

# Install wasm32 target and dx CLI
RUN rustup target add wasm32-unknown-unknown \
    && cargo install dioxus-cli@0.7.3

WORKDIR /app

# Copy full source (cache mounts make stub tricks unnecessary)
COPY Cargo.toml Cargo.lock ./
COPY server/ server/
COPY shared/ shared/
# Stub out client members so workspace resolves
COPY client/cli/Cargo.toml client/cli/Cargo.toml
RUN mkdir -p client/cli/src && echo "fn main() {}" > client/cli/src/main.rs
COPY client/desktop/Cargo.toml client/desktop/Cargo.toml
RUN mkdir -p client/desktop/src && echo "fn main() {}" > client/desktop/src/main.rs
COPY client/local/Cargo.toml client/local/Cargo.toml
RUN mkdir -p client/local/src && echo "" > client/local/src/lib.rs

# Build the frontend WASM bundle with persistent Cargo caches
# Disable debug symbols so wasm-opt does not crash on DWARF data from cached artifacts.
# Also purge stale wasm build outputs so the flag takes effect even with a warm cache.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    rm -rf /app/target/wasm32-unknown-unknown/release/deps/savhub_frontend* \
    && cd server/frontend && dx build --release --debug-symbols false \
    && mkdir -p /app/dist \
    && cp -r /app/target/dx/savhub-frontend/release/web/public /app/dist/public

# ── Stage 2: Build backend ────────────────────────────────────────────
FROM rust:1.94-bookworm AS backend-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev cmake g++ make perl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY docs/ docs/
COPY server/ server/
COPY shared/ shared/
# Stub out non-server members so workspace resolves
COPY client/cli/Cargo.toml client/cli/Cargo.toml
RUN mkdir -p client/cli/src && echo "fn main() {}" > client/cli/src/main.rs
COPY client/desktop/Cargo.toml client/desktop/Cargo.toml
RUN mkdir -p client/desktop/src && echo "fn main() {}" > client/desktop/src/main.rs
COPY client/local/Cargo.toml client/local/Cargo.toml
RUN mkdir -p client/local/src && echo "" > client/local/src/lib.rs

# Build the backend binary with persistent Cargo caches
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release -p savhub-backend \
    && cp /app/target/release/savhub-backend /app/savhub-backend

# ── Stage 3: Runtime ──────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl git openssh-client \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid 1000 savhub \
    && useradd --uid 1000 --gid savhub --shell /bin/false savhub

COPY --from=backend-builder /app/savhub-backend /app/savhub-backend

# Copy frontend build assets into the static directory
COPY --from=frontend-builder /app/dist/public /app/static

RUN chown -R savhub:savhub /app

USER savhub
WORKDIR /app

ENV SAVHUB_BIND=0.0.0.0:5006
EXPOSE 5006

HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:5006/api/v1/health || exit 1

ENTRYPOINT ["/app/savhub-backend"]

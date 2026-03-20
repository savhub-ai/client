# Savhub development commands

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]
client_root := "E:\\Works\\savhub-ai\\savhub-client"

# Start PostgreSQL via Docker
db:
    docker compose up -d postgres

# Stop PostgreSQL
db-stop:
    docker compose down

# Copy .env.example to .env (skips if .env already exists)
env:
    if (!(Test-Path .env)) { Copy-Item .env.example .env }

# Run backend
backend:
    cargo run -p savhub-backend

# Run frontend dev server
frontend:
    cd frontend; dx serve --platform web --port 5007

# Build frontend (release)
frontend-build:
    cd frontend; dx build --release

# Check entire workspace
check:
    cargo check --workspace

# Format entire workspace
fmt:
    cargo fmt --all

# Verify formatting without writing changes
fmt-check:
    cargo fmt --all --check

# Test entire workspace
test:
    cargo test --workspace

# Spellcheck source and docs
typos:
    typos

# Match the main GitHub Actions quality gates locally
ci: fmt-check typos check test

# Setup: start DB + copy env
setup: env db

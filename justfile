# Savhub development commands

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

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
    cd web; dx serve --platform web --port 5007

# Build frontend (release)
frontend-build:
    cd web; dx build --release

# Run desktop app (debug)
desktop:
    cargo run -p savhub-desktop

# Run desktop app (release)
desktop-release:
    cargo run -p savhub-desktop --release

# Run CLI command
cli *ARGS:
    cargo run -p savhub -- {{ARGS}}

# Check entire workspace
check:
    cargo check --workspace

# Format entire workspace
fmt:
    cargo fmt --all

# Verify formatting without writing changes
fmt-check:
    cargo fmt --all --check

# Lint entire workspace
lint:
    cargo clippy --workspace

# Test entire workspace
test:
    cargo test --workspace

# Build the CLI and copy it to ~/bin for local testing
dist-dev:
    cargo build -p savhub
    $binDir = Join-Path $env:USERPROFILE "bin"; New-Item -ItemType Directory -Force -Path $binDir | Out-Null; $src = Join-Path "{{justfile_directory()}}" "target\debug\savhub.exe"; Copy-Item $src (Join-Path $binDir "savhub.exe") -Force; Write-Host "Copied to $binDir\savhub.exe"

# Build entire workspace
build:
    cargo build --workspace

# Build entire workspace (release)
build-release:
    cargo build --workspace --release

# Spellcheck source and docs
typos:
    typos

# Match the main GitHub Actions quality gates locally
ci: fmt-check typos check test

# Setup: start DB + copy env
setup: env db

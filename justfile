# Savhub Client — development commands

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# Run the desktop app in debug mode
desktop:
    cargo run -p savhub-desktop

# Run the desktop app in release mode
desktop-release:
    cargo run -p savhub-desktop --release

# Run a CLI command (e.g. `just cli search foo`)
cli *ARGS:
    cargo run -p savhub -- {{ARGS}}

# Build the full workspace
build:
    cargo build --workspace

# Build release binaries
build-release:
    cargo build --workspace --release

# Build the CLI and copy it to ~/bin for local testing
dist-dev:
    cargo build -p savhub
    $binDir = Join-Path $env:USERPROFILE "bin"; New-Item -ItemType Directory -Force -Path $binDir | Out-Null; $src = Join-Path "{{justfile_directory()}}" "target\debug\savhub.exe"; Copy-Item $src (Join-Path $binDir "savhub.exe") -Force; Write-Host "Copied to $binDir\savhub.exe"

# Check compilation without building
check:
    cargo check --workspace

# Run clippy lints
lint:
    cargo clippy --workspace

# Format code
fmt:
    cargo fmt --all

# Run the MCP server in debug mode
mcp *ARGS:
    cargo run -p savhub-mcp -- {{ARGS}}

# Build the MCP server in release mode
mcp-release:
    cargo build -p savhub-mcp --release

# Check formatting
fmt-check:
    cargo fmt --all -- --check

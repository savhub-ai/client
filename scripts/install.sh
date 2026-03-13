#!/usr/bin/env bash
# Savhub CLI installer for Linux and macOS.
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/savhub-ai/savhub-client/main/scripts/install.sh | bash
#   curl -fsSL ... | bash -s -- --version v0.2.0
#   curl -fsSL ... | bash -s -- --install-dir /custom/path

set -euo pipefail

REPO="savhub-ai/client"
INSTALL_DIR="${HOME}/.savhub/bin"
VERSION=""

usage() {
    echo "Usage: install.sh [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --version VERSION   Install a specific version (e.g. v0.2.0)"
    echo "  --install-dir DIR   Install to a custom directory (default: ~/.savhub/bin)"
    echo "  --help              Show this help"
    exit 0
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION="$2"; shift 2 ;;
        --install-dir) INSTALL_DIR="$2"; shift 2 ;;
        --help) usage ;;
        *) echo "Unknown option: $1"; usage ;;
    esac
done

detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os="linux" ;;
        Darwin) os="macos" ;;
        *)      echo "Error: unsupported OS: $os"; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64)   arch="x64" ;;
        aarch64|arm64)  arch="arm64" ;;
        *)              echo "Error: unsupported architecture: $arch"; exit 1 ;;
    esac

    echo "${os}-${arch}"
}

get_latest_version() {
    local url="https://api.github.com/repos/${REPO}/releases/latest"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" | grep '"tag_name"' | sed -E 's/.*"tag_name":\s*"([^"]+)".*/\1/'
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "$url" | grep '"tag_name"' | sed -E 's/.*"tag_name":\s*"([^"]+)".*/\1/'
    else
        echo "Error: curl or wget is required" >&2
        exit 1
    fi
}

download() {
    local url="$1" dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL -o "$dest" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$dest" "$url"
    fi
}

main() {
    local platform
    platform="$(detect_platform)"
    echo "Detected platform: ${platform}"

    if [ -z "$VERSION" ]; then
        echo "Fetching latest version..."
        VERSION="$(get_latest_version)"
        if [ -z "$VERSION" ]; then
            echo "Error: could not determine latest version"
            exit 1
        fi
    fi
    echo "Installing savhub ${VERSION}..."

    local archive_name="savhub-cli-${platform}.tar.gz"
    local download_url="https://github.com/${REPO}/releases/download/${VERSION}/${archive_name}"

    local tmp_dir
    tmp_dir="$(mktemp -d)"
    trap 'rm -rf "$tmp_dir"' EXIT

    echo "Downloading ${download_url}..."
    download "$download_url" "${tmp_dir}/${archive_name}"

    echo "Extracting..."
    tar -xzf "${tmp_dir}/${archive_name}" -C "$tmp_dir"

    mkdir -p "$INSTALL_DIR"

    # Find the binary inside the extracted directory
    local binary_path
    binary_path="$(find "$tmp_dir" -name "savhub" -type f | head -1)"
    if [ -z "$binary_path" ]; then
        echo "Error: savhub binary not found in archive"
        exit 1
    fi

    cp "$binary_path" "${INSTALL_DIR}/savhub"
    chmod +x "${INSTALL_DIR}/savhub"

    echo "Installed savhub to ${INSTALL_DIR}/savhub"

    # Add to PATH if not already present
    add_to_path

    echo ""
    echo "savhub ${VERSION} installed successfully!"
    echo ""
    if ! command -v savhub >/dev/null 2>&1; then
        echo "Restart your shell or run:"
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi
}

add_to_path() {
    # Check if already in PATH
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) return ;;
    esac

    local line="export PATH=\"${INSTALL_DIR}:\$PATH\""
    local shell_name
    shell_name="$(basename "${SHELL:-/bin/sh}")"

    local profiles=()
    case "$shell_name" in
        zsh)
            profiles=("$HOME/.zshrc")
            ;;
        bash)
            if [ -f "$HOME/.bash_profile" ]; then
                profiles=("$HOME/.bash_profile")
            elif [ -f "$HOME/.bashrc" ]; then
                profiles=("$HOME/.bashrc")
            else
                profiles=("$HOME/.profile")
            fi
            ;;
        fish)
            local fish_conf="${HOME}/.config/fish/conf.d"
            mkdir -p "$fish_conf"
            if ! grep -q "${INSTALL_DIR}" "${fish_conf}/savhub.fish" 2>/dev/null; then
                echo "set -gx PATH ${INSTALL_DIR} \$PATH" >> "${fish_conf}/savhub.fish"
                echo "Added ${INSTALL_DIR} to fish PATH"
            fi
            return
            ;;
        *)
            profiles=("$HOME/.profile")
            ;;
    esac

    for profile in "${profiles[@]}"; do
        if [ -f "$profile" ] && grep -q "${INSTALL_DIR}" "$profile" 2>/dev/null; then
            continue
        fi
        { echo ""; echo "# Savhub CLI"; echo "$line"; } >> "$profile"
        echo "Added ${INSTALL_DIR} to PATH in ${profile}"
    done
}

main

#!/usr/bin/env bash
set -euo pipefail

# Meda Release Installer Script
# Downloads and installs the latest meda binary from GitHub releases

# Configuration
REPO="cirunlabs/meda"
GITHUB_API="https://api.github.com"
GITHUB_RELEASES="https://github.com/${REPO}/releases"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

detect_platform() {
    # Check OS
    if [[ "$(uname -s)" != "Linux" ]]; then
        print_error "This software only supports Linux"
        exit 1
    fi

    # Detect architecture
    case "$(uname -m)" in
        x86_64|amd64)   echo "Linux_x86_64" ;;
        aarch64|arm64)  echo "Linux_arm64" ;;
        *)
            print_error "Unsupported architecture: $(uname -m)"
            print_error "Only x86_64 and arm64 are supported"
            exit 1
            ;;
    esac
}

get_latest_version() {
    local latest_url="${GITHUB_API}/repos/${REPO}/releases/latest"
    local version

    if command -v curl >/dev/null 2>&1; then
        version=$(curl -s "$latest_url" | grep '"tag_name"' | sed -E 's/.*"tag_name": "([^"]+)".*/\1/')
    elif command -v wget >/dev/null 2>&1; then
        version=$(wget -qO- "$latest_url" | grep '"tag_name"' | sed -E 's/.*"tag_name": "([^"]+)".*/\1/')
    else
        print_error "Either curl or wget is required to download meda" >&2
        exit 1
    fi

    if [[ -z "$version" ]]; then
        print_error "Failed to get latest version from GitHub API" >&2
        exit 1
    fi

    echo "$version"
}

download_and_install() {
    local version="$1"
    local platform="$2"
    local temp_dir

    temp_dir=$(mktemp -d)
    trap "rm -rf $temp_dir" EXIT

    # Construct download URL - platform is already in correct format (Linux_x86_64, Linux_arm64)
    local binary_name="meda"
    local archive_name="meda_${platform}.tar.gz"
    local download_url="${GITHUB_RELEASES}/download/${version}/${archive_name}"

    print_status "Downloading meda ${version} for ${platform}..."
    print_status "URL: ${download_url}"

    local archive_path="${temp_dir}/${archive_name}"

    if command -v curl >/dev/null 2>&1; then
        if ! curl -L --fail --progress-bar "$download_url" -o "$archive_path"; then
            print_error "Failed to download meda from $download_url"
            print_error "This might mean the release doesn't have binaries for your platform: $platform"
            exit 1
        fi
    elif command -v wget >/dev/null 2>&1; then
        if ! wget --progress=bar:force "$download_url" -O "$archive_path"; then
            print_error "Failed to download meda from $download_url"
            print_error "This might mean the release doesn't have binaries for your platform: $platform"
            exit 1
        fi
    fi

    # Extract the archive
    print_status "Extracting archive..."
    tar -xzf "$archive_path" -C "$temp_dir"

    # Find the binary
    local binary_path
    if [[ -f "${temp_dir}/${binary_name}" ]]; then
        binary_path="${temp_dir}/${binary_name}"
    else
        print_error "Could not find meda binary in the downloaded archive"
        print_status "Contents of archive:"
        ls -la "$temp_dir"
        exit 1
    fi

    # Install the binary
    install_binary "$binary_path"
}

install_binary() {
    local binary_path="$1"
    local dest_path="${INSTALL_DIR}/meda"

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    print_status "Installing meda to $dest_path"

    # Copy binary
    cp "$binary_path" "$dest_path"
    chmod +x "$dest_path"

    # Verify installation
    if [[ -x "$dest_path" ]]; then
        print_success "Meda installed successfully to $dest_path"
    else
        print_error "Installation failed - binary is not executable"
        exit 1
    fi
}

check_path() {
    # Check if install directory is in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        print_warning "Warning: $INSTALL_DIR is not in your PATH"
        print_warning "Add the following line to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        echo -e "${BLUE}export PATH=\"$INSTALL_DIR:\$PATH\"${NC}"
        print_warning "Then reload your shell or run: source ~/.bashrc"
    else
        print_success "$INSTALL_DIR is already in your PATH"
    fi
}

verify_installation() {
    print_status "Verifying installation..."

    local dest_path="${INSTALL_DIR}/meda"
    if [[ -x "$dest_path" ]]; then
        local version
        version=$("$dest_path" --version 2>/dev/null || echo "unknown")
        print_success "Meda CLI is available: $version"
        print_status "Try running: meda --help"
    else
        print_warning "meda command verification failed"
        print_status "You can run it directly: $dest_path --help"
    fi
}

show_usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Downloads and installs the latest meda CLI from GitHub releases

OPTIONS:
    -h, --help          Show this help message
    -v, --version       Install specific version (e.g., v0.2.0)
    -i, --install-dir   Set custom installation directory (default: ~/.local/bin)

ENVIRONMENT VARIABLES:
    INSTALL_DIR         Override default installation directory

EXAMPLES:
    # Install latest version
    $0

    # Install specific version
    $0 --version v0.2.0

    # Install to custom directory
    $0 --install-dir /usr/local/bin

    # Using environment variable
    INSTALL_DIR=/opt/bin $0
EOF
}

main() {
    local version=""

    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            -h|--help)
                show_usage
                exit 0
                ;;
            -v|--version)
                version="$2"
                shift 2
                ;;
            -i|--install-dir)
                INSTALL_DIR="$2"
                shift 2
                ;;
            *)
                print_error "Unknown option: $1"
                show_usage
                exit 1
                ;;
        esac
    done

    print_status "Meda Release Installer"
    print_status "Repository: $REPO"
    print_status "Install directory: $INSTALL_DIR"
    echo

    # Detect platform
    local platform
    platform=$(detect_platform)
    print_status "Detected platform: $platform"

    # Get version to install
    if [[ -z "$version" ]]; then
        print_status "Fetching latest release information..."
        version=$(get_latest_version)
    fi
    print_status "Target version: $version"

    # Download and install
    download_and_install "$version" "$platform"

    check_path
    verify_installation

    echo
    print_success "Installation completed successfully!"
    print_status "Run 'meda --help' to get started"
}

# Run the main function
main "$@"
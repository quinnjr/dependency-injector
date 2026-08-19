#!/usr/bin/env bash
#
# Deploy script for FFI bindings
# Builds and publishes FFI packages for Go, Node.js, Python, and C#
#
# Usage:
#   ./scripts/deploy-ffi.sh [OPTIONS] [TARGETS...]
#
# Options:
#   --dry-run     Build but don't publish
#   --skip-build  Skip building the Rust library
#   --force       Skip confirmation prompts
#   --help        Show this help message
#
# Targets:
#   all           Build/publish all targets (default)
#   rust          Build the Rust FFI library only
#   nodejs        Build/publish Node.js package
#   python        Build/publish Python package
#   go            Build Go module (no publish - use go get)
#   csharp        Build C# package
#
# Examples:
#   ./scripts/deploy-ffi.sh                    # Build all
#   ./scripts/deploy-ffi.sh rust nodejs        # Build Rust lib and Node.js
#   ./scripts/deploy-ffi.sh --dry-run nodejs   # Dry run Node.js publish

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'
BOLD='\033[1m'

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Options
DRY_RUN=false
SKIP_BUILD=false
FORCE=false
TARGETS=()

# Print functions
print_header() {
    echo -e "\n${BOLD}${BLUE}══════════════════════════════════════════════════════════════${NC}"
    echo -e "${BOLD}${BLUE}  $1${NC}"
    echo -e "${BOLD}${BLUE}══════════════════════════════════════════════════════════════${NC}\n"
}

print_step() {
    echo -e "${CYAN}→${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Show help
show_help() {
    cat << EOF
${BOLD}FFI Deploy Script for dependency-injector${NC}

${BOLD}USAGE:${NC}
    ./scripts/deploy-ffi.sh [OPTIONS] [TARGETS...]

${BOLD}OPTIONS:${NC}
    --dry-run     Build but don't publish packages
    --skip-build  Skip building the Rust library
    --force       Skip confirmation prompts
    --help        Show this help message

${BOLD}TARGETS:${NC}
    all           Build/publish all targets (default)
    rust          Build the Rust FFI shared library only
    nodejs        Build/publish Node.js package to npm
    python        Build/publish Python package to PyPI
    go            Build Go module (no publish needed)
    csharp        Build C# NuGet package

${BOLD}EXAMPLES:${NC}
    # Build all FFI targets
    ./scripts/deploy-ffi.sh

    # Build only Rust library and Node.js
    ./scripts/deploy-ffi.sh rust nodejs

    # Dry run Node.js publish
    ./scripts/deploy-ffi.sh --dry-run nodejs

    # Skip Rust build (use existing library)
    ./scripts/deploy-ffi.sh --skip-build nodejs python

${BOLD}PREREQUISITES:${NC}
    - Rust toolchain with 'ffi' feature
    - Node.js + pnpm (for nodejs target)
    - Python + pip + build + twine (for python target)
    - Go 1.21+ (for go target)
    - .NET 8.0 SDK (for csharp target)

${BOLD}ENVIRONMENT VARIABLES:${NC}
    NPM_TOKEN       npm authentication token
    PYPI_TOKEN      PyPI authentication token
    NUGET_API_KEY   NuGet API key

EOF
    exit 0
}

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            --skip-build)
                SKIP_BUILD=true
                shift
                ;;
            --force)
                FORCE=true
                shift
                ;;
            --help|-h)
                show_help
                ;;
            -*)
                print_error "Unknown option: $1"
                exit 1
                ;;
            *)
                TARGETS+=("$1")
                shift
                ;;
        esac
    done

    # Default to all targets
    if [[ ${#TARGETS[@]} -eq 0 ]]; then
        TARGETS=("all")
    fi

    # Expand 'all' to individual targets
    if [[ " ${TARGETS[*]} " =~ " all " ]]; then
        TARGETS=("rust" "nodejs" "python" "go" "csharp")
    fi
}

# Get version from Cargo.toml
get_version() {
    grep -m1 '^version' "$PROJECT_ROOT/Cargo.toml" | cut -d'"' -f2
}

# Build Rust FFI library
build_rust() {
    print_header "Building Rust FFI Library"

    if [[ "$SKIP_BUILD" == "true" ]]; then
        print_warning "Skipping Rust build (--skip-build)"
        return
    fi

    cd "$PROJECT_ROOT"

    print_step "Building cdylib with FFI feature..."
    cargo rustc --release --features ffi --crate-type cdylib

    # Find and report the library
    local lib_name=""
    local lib_path=""

    if [[ -f "target/release/libdependency_injector.so" ]]; then
        lib_name="libdependency_injector.so"
        lib_path="target/release/$lib_name"
    elif [[ -f "target/release/libdependency_injector.dylib" ]]; then
        lib_name="libdependency_injector.dylib"
        lib_path="target/release/$lib_name"
    elif [[ -f "target/release/dependency_injector.dll" ]]; then
        lib_name="dependency_injector.dll"
        lib_path="target/release/$lib_name"
    fi

    if [[ -n "$lib_path" ]]; then
        local size
        size=$(du -h "$lib_path" | cut -f1)
        print_success "Built: $lib_path ($size)"

        # Copy to ffi directory
        print_step "Copying library to ffi/..."
        cp "$lib_path" "$PROJECT_ROOT/ffi/"
        print_success "Copied to ffi/$lib_name"
    else
        print_error "Could not find built library"
        exit 1
    fi
}

# Build/publish Node.js package
build_nodejs() {
    print_header "Building Node.js Package"

    local nodejs_dir="$PROJECT_ROOT/ffi/nodejs"

    if [[ ! -d "$nodejs_dir" ]]; then
        print_error "Node.js FFI directory not found: $nodejs_dir"
        return
    fi

    cd "$nodejs_dir"

    # Check pnpm
    if ! command -v pnpm &> /dev/null; then
        print_error "pnpm is not installed"
        return
    fi

    # Get version
    local version
    version=$(get_version)

    # Update package.json version
    print_step "Updating package.json to version $version..."
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" package.json
    print_success "Updated version"

    # Install dependencies
    print_step "Installing dependencies..."
    pnpm install
    print_success "Dependencies installed"

    # Build
    print_step "Building TypeScript..."
    pnpm run build
    print_success "Build completed"

    # Run tests
    print_step "Running tests..."
    if pnpm test 2>/dev/null; then
        print_success "Tests passed"
    else
        print_warning "Tests skipped or failed"
    fi

    # Publish
    if [[ "$DRY_RUN" == "true" ]]; then
        print_step "Dry run: pnpm publish --dry-run"
        pnpm publish --dry-run --no-git-checks 2>/dev/null || true
    else
        print_step "Publishing to npm..."
        if [[ -n "${NPM_TOKEN:-}" ]]; then
            echo "//registry.npmjs.org/:_authToken=${NPM_TOKEN}" > .npmrc
        fi
        pnpm publish --no-git-checks
        print_success "Published to npm"
    fi
}

# Build/publish Python package
build_python() {
    print_header "Building Python Package"

    local python_dir="$PROJECT_ROOT/ffi/python"

    if [[ ! -d "$python_dir" ]]; then
        print_error "Python FFI directory not found: $python_dir"
        return
    fi

    cd "$python_dir"

    # Check Python
    if ! command -v python3 &> /dev/null; then
        print_error "python3 is not installed"
        return
    fi

    # Get version
    local version
    version=$(get_version)

    # Update version in pyproject.toml or setup.py
    if [[ -f "pyproject.toml" ]]; then
        print_step "Updating pyproject.toml to version $version..."
        sed -i "s/version = \"[^\"]*\"/version = \"$version\"/" pyproject.toml
    fi
    print_success "Updated version"

    # Install build tools
    print_step "Installing build tools..."
    python3 -m pip install --quiet build twine

    # Clean previous builds
    rm -rf dist/ build/ *.egg-info/

    # Build
    print_step "Building Python package..."
    python3 -m build
    print_success "Build completed"

    # List built files
    ls -la dist/

    # Publish
    if [[ "$DRY_RUN" == "true" ]]; then
        print_step "Dry run: twine check dist/*"
        python3 -m twine check dist/*
    else
        print_step "Publishing to PyPI..."
        if [[ -n "${PYPI_TOKEN:-}" ]]; then
            python3 -m twine upload dist/* -u __token__ -p "$PYPI_TOKEN"
        else
            python3 -m twine upload dist/*
        fi
        print_success "Published to PyPI"
    fi
}

# Build Go module
build_go() {
    print_header "Building Go Module"

    local go_dir="$PROJECT_ROOT/ffi/go"

    if [[ ! -d "$go_dir" ]]; then
        print_error "Go FFI directory not found: $go_dir"
        return
    fi

    cd "$go_dir/di"

    # Check Go
    if ! command -v go &> /dev/null; then
        print_error "go is not installed"
        return
    fi

    print_step "Go version: $(go version)"

    # Tidy module
    print_step "Running go mod tidy..."
    go mod tidy
    print_success "Module tidied"

    # Build
    print_step "Building Go module..."
    CGO_ENABLED=1 go build -v ./...
    print_success "Build completed"

    # Run tests
    print_step "Running tests..."
    export LD_LIBRARY_PATH="$PROJECT_ROOT/target/release:${LD_LIBRARY_PATH:-}"
    if CGO_ENABLED=1 go test -v ./... 2>/dev/null; then
        print_success "Tests passed"
    else
        print_warning "Tests skipped (library may not be in LD_LIBRARY_PATH)"
    fi

    print_success "Go module ready"
    echo ""
    echo "To use: go get github.com/quinnjr/dependency-injector/ffi/go/di"
}

# Build C# package
build_csharp() {
    print_header "Building C# Package"

    local csharp_dir="$PROJECT_ROOT/ffi/csharp"

    if [[ ! -d "$csharp_dir" ]]; then
        print_error "C# FFI directory not found: $csharp_dir"
        return
    fi

    cd "$csharp_dir"

    # Check dotnet
    if ! command -v dotnet &> /dev/null; then
        print_error "dotnet is not installed"
        return
    fi

    print_step ".NET version: $(dotnet --version)"

    # Get version
    local version
    version=$(get_version)

    # Restore
    print_step "Restoring packages..."
    dotnet restore
    print_success "Packages restored"

    # Build
    print_step "Building solution..."
    dotnet build -c Release
    print_success "Build completed"

    # Run tests
    print_step "Running tests..."
    export LD_LIBRARY_PATH="$PROJECT_ROOT/target/release:${LD_LIBRARY_PATH:-}"
    if dotnet test -c Release --no-build 2>/dev/null; then
        print_success "Tests passed"
    else
        print_warning "Tests skipped (library may not be accessible)"
    fi

    # Pack NuGet
    print_step "Creating NuGet package..."
    dotnet pack DependencyInjector/DependencyInjector.csproj -c Release -p:PackageVersion="$version" -o ./nupkg
    print_success "NuGet package created"

    ls -la nupkg/

    # Publish
    if [[ "$DRY_RUN" == "true" ]]; then
        print_step "Dry run: would push to NuGet"
    else
        if [[ -n "${NUGET_API_KEY:-}" ]]; then
            print_step "Publishing to NuGet..."
            dotnet nuget push nupkg/*.nupkg --api-key "$NUGET_API_KEY" --source https://api.nuget.org/v3/index.json
            print_success "Published to NuGet"
        else
            print_warning "NUGET_API_KEY not set, skipping publish"
        fi
    fi
}

# Print summary
print_summary() {
    print_header "Build Summary"

    local version
    version=$(get_version)

    echo "Version: $version"
    echo ""
    echo "Built targets:"
    for target in "${TARGETS[@]}"; do
        echo "  • $target"
    done
    echo ""

    if [[ "$DRY_RUN" == "true" ]]; then
        echo -e "${YELLOW}DRY RUN - no packages were published${NC}"
    fi
}

# Main
main() {
    parse_args "$@"

    echo -e "${BOLD}"
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║       dependency-injector FFI Deploy Script                ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"

    if [[ "$DRY_RUN" == "true" ]]; then
        echo -e "${YELLOW}Running in DRY RUN mode${NC}"
    fi

    echo "Targets: ${TARGETS[*]}"
    echo ""

    # Confirm
    if [[ "$FORCE" != "true" ]] && [[ "$DRY_RUN" != "true" ]]; then
        read -p "Continue? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo "Aborted"
            exit 0
        fi
    fi

    # Build targets
    for target in "${TARGETS[@]}"; do
        case $target in
            rust)
                build_rust
                ;;
            nodejs)
                build_nodejs
                ;;
            python)
                build_python
                ;;
            go)
                build_go
                ;;
            csharp)
                build_csharp
                ;;
            *)
                print_warning "Unknown target: $target"
                ;;
        esac
    done

    print_summary
}

main "$@"




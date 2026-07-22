#!/usr/bin/env bash
#
# Deploy script for dependency-injector
# Publishes the Rust library to crates.io
#
# Usage:
#   ./scripts/deploy.sh [OPTIONS] <VERSION>
#
# Options:
#   --dry-run     Perform all checks but don't publish
#   --skip-tests  Skip running tests (not recommended)
#   --skip-derive Skip publishing dependency-injector-derive
#   --force       Skip confirmation prompts
#   --help        Show this help message
#
# Examples:
#   ./scripts/deploy.sh 0.2.2
#   ./scripts/deploy.sh --dry-run 0.2.2
#   ./scripts/deploy.sh --skip-derive 0.2.2

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Default options
DRY_RUN=false
SKIP_TESTS=false
SKIP_DERIVE=false
FORCE=false
VERSION=""

# Print colored output
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
${BOLD}Deploy Script for dependency-injector${NC}

${BOLD}USAGE:${NC}
    ./scripts/deploy.sh [OPTIONS] <VERSION>

${BOLD}ARGUMENTS:${NC}
    <VERSION>    Semantic version to release (e.g., 0.2.2)

${BOLD}OPTIONS:${NC}
    --dry-run     Perform all checks but don't publish or push
    --skip-tests  Skip running tests (not recommended)
    --skip-derive Skip publishing dependency-injector-derive
    --force       Skip confirmation prompts
    --help        Show this help message

${BOLD}EXAMPLES:${NC}
    # Full release
    ./scripts/deploy.sh 0.2.2

    # Test the release process without publishing
    ./scripts/deploy.sh --dry-run 0.2.2

    # Release only the main crate (if derive hasn't changed)
    ./scripts/deploy.sh --skip-derive 0.2.2

${BOLD}REQUIREMENTS:${NC}
    - Git with clean working directory
    - Cargo with crates.io authentication (cargo login)
    - On 'develop' branch
    - All commits pushed

${BOLD}WORKFLOW:${NC}
    1. Validates environment and version
    2. Runs tests, clippy, and format check
    3. Updates version in Cargo.toml files
    4. Updates CHANGELOG.md
    5. Commits and merges to main
    6. Creates git tag
    7. Pushes to origin
    8. Publishes to crates.io
    9. Builds FFI artifacts

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
            --skip-tests)
                SKIP_TESTS=true
                shift
                ;;
            --skip-derive)
                SKIP_DERIVE=true
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
                echo "Use --help for usage information"
                exit 1
                ;;
            *)
                if [[ -z "$VERSION" ]]; then
                    VERSION="$1"
                else
                    print_error "Unexpected argument: $1"
                    exit 1
                fi
                shift
                ;;
        esac
    done

    if [[ -z "$VERSION" ]]; then
        print_error "Version is required"
        echo "Usage: ./scripts/deploy.sh [OPTIONS] <VERSION>"
        exit 1
    fi

    # Validate version format
    if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
        print_error "Invalid version format: $VERSION"
        echo "Expected format: MAJOR.MINOR.PATCH (e.g., 0.2.2 or 1.0.0-beta.1)"
        exit 1
    fi
}

# Check prerequisites
check_prerequisites() {
    print_header "Checking Prerequisites"

    # Check we're in the project root
    if [[ ! -f "$PROJECT_ROOT/Cargo.toml" ]]; then
        print_error "Must be run from project root"
        exit 1
    fi
    print_success "In project root"

    # Check git
    if ! command -v git &> /dev/null; then
        print_error "git is not installed"
        exit 1
    fi
    print_success "git is installed"

    # Check cargo
    if ! command -v cargo &> /dev/null; then
        print_error "cargo is not installed"
        exit 1
    fi
    print_success "cargo is installed"

    # Check current branch
    local current_branch
    current_branch=$(git rev-parse --abbrev-ref HEAD)
    if [[ "$current_branch" != "develop" ]]; then
        print_error "Must be on 'develop' branch (currently on '$current_branch')"
        exit 1
    fi
    print_success "On develop branch"

    # Check for uncommitted changes
    if ! git diff --quiet HEAD; then
        print_error "Working directory has uncommitted changes"
        echo "Please commit or stash your changes before deploying"
        exit 1
    fi
    print_success "Working directory is clean"

    # Check for unpushed commits
    local unpushed
    unpushed=$(git log origin/develop..HEAD --oneline 2>/dev/null | wc -l)
    if [[ "$unpushed" -gt 0 ]]; then
        print_warning "There are $unpushed unpushed commits"
        if [[ "$FORCE" != "true" ]] && [[ "$DRY_RUN" != "true" ]]; then
            read -p "Continue anyway? [y/N] " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                exit 1
            fi
        fi
    else
        print_success "All commits pushed"
    fi

    # Check crates.io authentication
    if [[ "$DRY_RUN" != "true" ]]; then
        if ! cargo login --help &> /dev/null; then
            print_error "cargo login not available"
            exit 1
        fi
        print_success "cargo login available"
    fi

    # Get current versions
    local current_main_version
    local current_derive_version
    current_main_version=$(grep -m1 '^version' "$PROJECT_ROOT/Cargo.toml" | cut -d'"' -f2)
    current_derive_version=$(grep -m1 '^version' "$PROJECT_ROOT/dependency-injector-derive/Cargo.toml" | cut -d'"' -f2)

    echo ""
    print_step "Current versions:"
    echo "    dependency-injector:        $current_main_version"
    echo "    dependency-injector-derive: $current_derive_version"
    echo "    Target version:             $VERSION"
}

# Run tests and checks
run_checks() {
    print_header "Running Checks"

    cd "$PROJECT_ROOT"

    if [[ "$SKIP_TESTS" == "true" ]]; then
        print_warning "Skipping tests (--skip-tests)"
    else
        print_step "Running tests..."
        if ! cargo test --all-features; then
            print_error "Tests failed"
            exit 1
        fi
        print_success "All tests passed"
    fi

    print_step "Running clippy..."
    if ! cargo clippy --all-features -- -D warnings; then
        print_error "Clippy found issues"
        exit 1
    fi
    print_success "Clippy passed"

    print_step "Checking formatting..."
    if ! cargo fmt --check; then
        print_error "Code is not formatted. Run 'cargo fmt'"
        exit 1
    fi
    print_success "Formatting OK"

    print_step "Building documentation..."
    if ! cargo doc --no-deps --all-features; then
        print_error "Documentation build failed"
        exit 1
    fi
    print_success "Documentation builds"

    print_step "Checking package..."
    if ! cargo package --list -p dependency-injector > /dev/null; then
        print_error "Package check failed"
        exit 1
    fi
    print_success "Package check passed"
}

# Update version in Cargo.toml
update_version() {
    print_header "Updating Version"

    cd "$PROJECT_ROOT"

    if [[ "$DRY_RUN" == "true" ]]; then
        print_warning "DRY RUN: Would update version in Cargo.toml to $VERSION"
        if [[ "$SKIP_DERIVE" != "true" ]]; then
            print_warning "DRY RUN: Would update dependency-injector-derive/Cargo.toml to $VERSION"
            print_warning "DRY RUN: Would update derive dependency reference in Cargo.toml to $VERSION"
        fi
        print_warning "DRY RUN: Would update Cargo.lock (cargo update -p dependency-injector)"
        return
    fi

    # Update main crate version
    print_step "Updating dependency-injector to $VERSION..."
    sed -i "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" Cargo.toml
    print_success "Updated Cargo.toml"

    # Update derive crate version if needed
    if [[ "$SKIP_DERIVE" != "true" ]]; then
        print_step "Updating dependency-injector-derive to $VERSION..."
        sed -i "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" dependency-injector-derive/Cargo.toml

        # Update the dependency reference in main Cargo.toml
        sed -i "s/dependency-injector-derive = { version = \"[^\"]*\"/dependency-injector-derive = { version = \"$VERSION\"/" Cargo.toml
        print_success "Updated derive crate"
    fi

    # Update Cargo.lock
    print_step "Updating Cargo.lock..."
    cargo update -p dependency-injector
    print_success "Updated Cargo.lock"
}

# Update changelog
update_changelog() {
    print_header "Updating Changelog"

    local changelog="$PROJECT_ROOT/CHANGELOG.md"
    local today
    today=$(date +%Y-%m-%d)

    # Check if version already exists in changelog
    if grep -q "## \[$VERSION\]" "$changelog"; then
        print_success "Version $VERSION already in CHANGELOG.md"
        return
    fi

    if [[ "$DRY_RUN" == "true" ]]; then
        print_warning "DRY RUN: Would add version $VERSION ($today) entry to CHANGELOG.md"
        return
    fi

    print_step "Adding version $VERSION to CHANGELOG.md..."

    # Create a temporary file with the new entry
    local temp_file
    temp_file=$(mktemp)

    # Read until the first version entry and insert new version
    awk -v version="$VERSION" -v date="$today" '
    /^## \[/ && !done {
        print "## [" version "] - " date
        print ""
        print "### Changed"
        print "- Version bump to " version
        print ""
        done = 1
    }
    { print }
    ' "$changelog" > "$temp_file"

    mv "$temp_file" "$changelog"
    print_success "Updated CHANGELOG.md"

    print_warning "Please review and edit CHANGELOG.md with actual release notes"
    if [[ "$FORCE" != "true" ]] && [[ "$DRY_RUN" != "true" ]]; then
        read -p "Press Enter when ready to continue..."
    fi
}

# Commit and tag
commit_and_tag() {
    print_header "Creating Release Commit"

    cd "$PROJECT_ROOT"

    if [[ "$DRY_RUN" == "true" ]]; then
        print_warning "DRY RUN: Would commit and tag v$VERSION"
        return
    fi

    # Stage changes
    print_step "Staging changes..."
    git add Cargo.toml Cargo.lock CHANGELOG.md
    if [[ "$SKIP_DERIVE" != "true" ]]; then
        git add dependency-injector-derive/Cargo.toml
    fi
    print_success "Changes staged"

    # Commit
    print_step "Creating commit..."
    git commit -m "chore(release): bump version to v$VERSION"
    print_success "Commit created"

    # Merge to main
    print_step "Merging to main..."
    git checkout main
    git merge develop -m "Merge develop for v$VERSION release"
    print_success "Merged to main"

    # Create tag
    print_step "Creating tag v$VERSION..."
    git tag -a "v$VERSION" -m "Release v$VERSION"
    print_success "Tag created"
}

# Push to remote
push_to_remote() {
    print_header "Pushing to Remote"

    cd "$PROJECT_ROOT"

    if [[ "$DRY_RUN" == "true" ]]; then
        print_warning "DRY RUN: Would push main and tag v$VERSION"
        return
    fi

    print_step "Pushing main branch..."
    git push origin main
    print_success "Pushed main"

    print_step "Pushing tag v$VERSION..."
    git push origin "v$VERSION"
    print_success "Pushed tag"

    print_step "Switching back to develop..."
    git checkout develop
    git push origin develop
    print_success "Back on develop"
}

# Publish to crates.io
publish_crates() {
    print_header "Publishing to crates.io"

    cd "$PROJECT_ROOT"

    if [[ "$DRY_RUN" == "true" ]]; then
        print_step "Running dry-run publish..."

        if [[ "$SKIP_DERIVE" != "true" ]]; then
            print_step "Dry-run: dependency-injector-derive"
            cargo publish --dry-run -p dependency-injector-derive
        fi

        print_step "Dry-run: dependency-injector"
        cargo publish --dry-run -p dependency-injector

        print_success "Dry run completed successfully"
        return
    fi

    # Publish derive crate first (if not skipped)
    if [[ "$SKIP_DERIVE" != "true" ]]; then
        print_step "Publishing dependency-injector-derive..."
        cargo publish -p dependency-injector-derive
        print_success "Published dependency-injector-derive"

        # Wait for crates.io to index
        print_step "Waiting for crates.io to index (30s)..."
        sleep 30
    fi

    # Publish main crate
    print_step "Publishing dependency-injector..."
    cargo publish -p dependency-injector
    print_success "Published dependency-injector"
}

# Build FFI artifacts
build_ffi_artifacts() {
    print_header "Building FFI Artifacts"

    cd "$PROJECT_ROOT"

    print_step "Building FFI shared library..."
    cargo rustc --release --features ffi --crate-type cdylib
    print_success "FFI library built"

    local lib_path=""
    if [[ -f "target/release/libdependency_injector.so" ]]; then
        lib_path="target/release/libdependency_injector.so"
    elif [[ -f "target/release/libdependency_injector.dylib" ]]; then
        lib_path="target/release/libdependency_injector.dylib"
    elif [[ -f "target/release/dependency_injector.dll" ]]; then
        lib_path="target/release/dependency_injector.dll"
    fi

    if [[ -n "$lib_path" ]]; then
        local size
        size=$(du -h "$lib_path" | cut -f1)
        print_success "FFI library: $lib_path ($size)"
    fi
}

# Print summary
print_summary() {
    print_header "Release Summary"

    if [[ "$DRY_RUN" == "true" ]]; then
        echo -e "${YELLOW}═══ DRY RUN COMPLETED ═══${NC}"
        echo ""
        echo "No changes were made. To perform the actual release, run:"
        echo "  ./scripts/deploy.sh $VERSION"
    else
        echo -e "${GREEN}═══ RELEASE v$VERSION COMPLETED ═══${NC}"
        echo ""
        echo "Published crates:"
        if [[ "$SKIP_DERIVE" != "true" ]]; then
            echo "  • dependency-injector-derive v$VERSION"
        fi
        echo "  • dependency-injector v$VERSION"
        echo ""
        echo "Links:"
        echo "  • https://crates.io/crates/dependency-injector"
        echo "  • https://docs.rs/dependency-injector"
        echo "  • https://github.com/pegasusheavy/dependency-injector/releases/tag/v$VERSION"
    fi
    echo ""
}

# Main function
main() {
    parse_args "$@"

    echo -e "${BOLD}"
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║       dependency-injector Deploy Script                    ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"

    if [[ "$DRY_RUN" == "true" ]]; then
        echo -e "${YELLOW}Running in DRY RUN mode - no changes will be made${NC}"
    fi

    echo "Version: $VERSION"
    echo ""

    # Confirm release
    if [[ "$FORCE" != "true" ]] && [[ "$DRY_RUN" != "true" ]]; then
        read -p "Ready to release v$VERSION? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo "Aborted"
            exit 0
        fi
    fi

    check_prerequisites
    run_checks
    update_version
    update_changelog
    commit_and_tag
    push_to_remote
    publish_crates
    build_ffi_artifacts
    print_summary
}

# Run main
main "$@"




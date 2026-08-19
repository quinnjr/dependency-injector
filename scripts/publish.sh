#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if a crate version is already published on crates.io
check_published() {
    local crate_name=$1
    local version=$2

    # Query crates.io API for the specific version
    local response=$(curl -s "https://crates.io/api/v1/crates/${crate_name}/${version}")

    # Check if version exists (response contains "version" field, not "errors")
    if echo "$response" | grep -q '"version"' && ! echo "$response" | grep -q '"errors"'; then
        return 0  # Already published
    else
        return 1  # Not published
    fi
}

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  dependency-injector Publish Script${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# Ensure we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" != "main" ]; then
    echo -e "${RED}Error: Must be on 'main' branch to publish.${NC}"
    echo -e "Current branch: $CURRENT_BRANCH"
    echo -e "Run: git checkout main"
    exit 1
fi

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
    echo -e "${RED}Error: Uncommitted changes detected.${NC}"
    echo -e "Please commit or stash your changes before publishing."
    exit 1
fi

# Get versions
MAIN_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
DERIVE_VERSION=$(grep '^version' dependency-injector-derive/Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

echo -e "Main crate version:   ${YELLOW}v${MAIN_VERSION}${NC}"
echo -e "Derive crate version: ${YELLOW}v${DERIVE_VERSION}${NC}"
echo ""

# Check if versions are already published
echo -e "Checking crates.io for existing versions..."

DERIVE_PUBLISHED=false
MAIN_PUBLISHED=false

if check_published "dependency-injector-derive" "$DERIVE_VERSION"; then
    echo -e "  dependency-injector-derive v${DERIVE_VERSION}: ${YELLOW}already published${NC}"
    DERIVE_PUBLISHED=true
else
    echo -e "  dependency-injector-derive v${DERIVE_VERSION}: ${GREEN}not published${NC}"
fi

if check_published "dependency-injector" "$MAIN_VERSION"; then
    echo -e "  dependency-injector v${MAIN_VERSION}: ${YELLOW}already published${NC}"
    MAIN_PUBLISHED=true
else
    echo -e "  dependency-injector v${MAIN_VERSION}: ${GREEN}not published${NC}"
fi

echo ""

# Exit if both are already published
if [ "$DERIVE_PUBLISHED" = true ] && [ "$MAIN_PUBLISHED" = true ]; then
    echo -e "${RED}Error: Both versions are already published on crates.io.${NC}"
    echo -e "Please bump the version numbers in Cargo.toml files."
    exit 1
fi

# Confirm
read -p "Publish these versions to crates.io? [y/N] " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Aborted.${NC}"
    exit 0
fi

echo ""
echo -e "${GREEN}Step 1/4: Running tests...${NC}"
cargo test --all-features
echo -e "${GREEN}✓ Tests passed${NC}"
echo ""

echo -e "${GREEN}Step 2/4: Running clippy...${NC}"
cargo clippy --all-features --all-targets -- -D warnings
echo -e "${GREEN}✓ Clippy passed${NC}"
echo ""

echo -e "${GREEN}Step 3/4: Publishing dependency-injector-derive v${DERIVE_VERSION}...${NC}"
if [ "$DERIVE_PUBLISHED" = true ]; then
    echo -e "${YELLOW}⊘ Skipping (already published)${NC}"
else
    cd dependency-injector-derive
    cargo publish
    cd ..
    echo -e "${GREEN}✓ dependency-injector-derive published${NC}"

    # Wait for crates.io to index the derive crate
    echo -e "${YELLOW}Waiting 30 seconds for crates.io to index...${NC}"
    sleep 30
fi
echo ""

echo -e "${GREEN}Step 4/4: Publishing dependency-injector v${MAIN_VERSION}...${NC}"
if [ "$MAIN_PUBLISHED" = true ]; then
    echo -e "${YELLOW}⊘ Skipping (already published)${NC}"
else
    cargo publish
    echo -e "${GREEN}✓ dependency-injector published${NC}"
fi
echo ""

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Successfully published!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "  dependency-injector-derive v${DERIVE_VERSION}"
echo -e "  dependency-injector v${MAIN_VERSION}"
echo ""
echo -e "Don't forget to:"
echo -e "  1. Create and push the tag: ${YELLOW}git tag -a v${MAIN_VERSION} -m \"Release v${MAIN_VERSION}\" && git push origin v${MAIN_VERSION}${NC}"
echo -e "  2. Create GitHub release"


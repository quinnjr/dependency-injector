# Development recipes for dependency-injector.
# Install just: https://github.com/casey/just

release_dir := justfile_directory() + "/target/release"
linux_lib := release_dir + "/libdependency_injector.so"

# List available recipes
default:
    @just --list

# Run the full test suite with all features
test:
    cargo test --all-features

# Formatting check + strict clippy (mirrors the pre-push hook's pedantic set)
lint:
    cargo fmt --all -- --check
    @# KEEP IN SYNC with the .cargo-husky/hooks/pre-push clippy flag list
    cargo clippy --all-targets --all-features -- \
        -D warnings \
        -D clippy::all \
        -D clippy::pedantic \
        -A clippy::module_name_repetitions \
        -A clippy::must_use_candidate \
        -A clippy::missing_errors_doc \
        -A clippy::missing_panics_doc \
        -A clippy::doc_markdown \
        -A clippy::return_self_not_must_use \
        -A clippy::single_match_else \
        -A clippy::cast_possible_truncation \
        -A clippy::uninlined_format_args \
        -A clippy::inline_always \
        -A clippy::ignored_unit_patterns \
        -A clippy::used_underscore_binding \
        -A clippy::missing_fields_in_debug \
        -A clippy::manual_let_else \
        -A clippy::elidable_lifetime_names \
        -A clippy::unreadable_literal \
        -A clippy::struct_excessive_bools \
        -A clippy::redundant_closure_for_method_calls \
        -A clippy::ptr_as_ptr \
        -A clippy::map_unwrap_or \
        -A clippy::manual_assert \
        -A clippy::cast_possible_wrap \
        -A clippy::bool_to_int_with_if \
        -A clippy::semicolon_if_nothing_returned \
        -A clippy::wildcard_imports

# Build the FFI shared library (cdylib)
build-ffi:
    cargo rustc --release --features ffi --crate-type cdylib

# Run all four FFI binding test suites against a fresh local cdylib.
# Linux paths shown; on macOS the loaders use DYLD_LIBRARY_PATH / .dylib.
test-bindings: build-ffi
    cd {{justfile_directory()}}/ffi/go/di && CGO_ENABLED=1 LD_LIBRARY_PATH={{release_dir}} go test -v ./...
    cd {{justfile_directory()}}/ffi/python && pip install -e ".[dev]" && DI_LIBRARY_PATH={{linux_lib}} python -m pytest
    cd {{justfile_directory()}}/ffi/nodejs && pnpm install --ignore-scripts && DI_LIBRARY_PATH={{linux_lib}} pnpm test
    cd {{justfile_directory()}}/ffi/csharp && LD_LIBRARY_PATH={{release_dir}} dotnet test

# Build the documentation site (Angular)
docs:
    cd {{justfile_directory()}}/docs && pnpm install && pnpm run build

# Run the internal Criterion benchmarks
bench:
    cargo bench --bench container_bench

# Run a fuzz target (requires nightly + cargo-fuzz)
fuzz target='fuzz_container':
    cargo +nightly fuzz run {{target}}

# (Re)install the git hooks from .cargo-husky/hooks/ via cargo-husky
hooks:
    cargo clean -p cargo-husky
    cargo test --no-run --all-features

# Contributing to dependency-injector

Thanks for contributing! This document describes the actual development loop for this
repository. Most day-to-day commands are also available as [`just`](https://github.com/casey/just)
recipes — see the [justfile](justfile) (`just` with no arguments lists them).

## Prerequisites

- **Rust** — MSRV is **1.85** (`rust-version` in [Cargo.toml](Cargo.toml), edition 2024).
  Develop on stable; CI checks the build against 1.85.0.
- **Rust nightly** + [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) — for fuzzing only.
- **Node.js ≥ 18 + pnpm 10** — for the docs site and the Node.js FFI binding.
- **Go ≥ 1.21**, **Python ≥ 3.10**, **.NET 8** — only if you touch the respective FFI binding.

## Getting started

```bash
git clone https://github.com/quinnjr/dependency-injector
cd dependency-injector
cargo test --all-features
```

The first `cargo test` also installs the git hooks (see below). Or run `just hooks`.

## Git hooks (cargo-husky)

Hooks live in [.cargo-husky/hooks/](.cargo-husky/hooks/) and are installed into
`.git/hooks/` automatically by [cargo-husky](https://github.com/rhysd/cargo-husky)
(a dev-dependency with the `user-hooks` feature) the first time its build script runs —
i.e. on your first `cargo test`. They are enforced locally and mirrored in CI:

- **pre-commit** (runs when `.rs` files are staged):
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --all-targets --all-features -- -D warnings`
  3. `cargo build --all-targets --all-features`
- **commit-msg** — enforces [Conventional Commits](https://www.conventionalcommits.org/):
  - Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`
  - Format: `<type>(<scope>): <description>` (scope optional for the hook, but repo
    convention is to include one — see [AGENTS.md](AGENTS.md))
  - Subject ≤ 72 characters, no trailing period, blank line before any body
- **pre-push**:
  1. `cargo test --all-features`
  2. `cargo clippy` with `-D clippy::all -D clippy::pedantic` plus a curated `-A` allow
     list — read [.cargo-husky/hooks/pre-push](.cargo-husky/hooks/pre-push) for the exact flags
  3. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`
  4. `cargo build --release --all-features`

If you edit a hook, force a reinstall with `just hooks`
(`cargo clean -p cargo-husky` + a rebuild).

## Testing

```bash
# The main suite (what CI and the pre-push hook run)
cargo test --all-features          # or: just test

# CI also tests these feature combinations — check them if you touch feature-gated code
cargo test --no-default-features
cargo test --features tracing
cargo test --features async
```

Unit tests go in `#[cfg(test)] mod tests` next to the code; integration tests in
[tests/](tests/); benchmarks in [benches/](benches/) using Criterion.

## Linting and formatting

```bash
just lint
```

`just lint` runs `cargo fmt --all -- --check` followed by clippy with the same strict
pedantic set the pre-push hook enforces (`-D clippy::all -D clippy::pedantic` plus the
curated `-A` allow list) — see the [justfile](justfile) for the exact flags. Passing
`just lint` means the clippy step of pre-push will pass too.

## FFI: building the shared library

The C ABI (and all language bindings) needs the crate built as a `cdylib`:

```bash
cargo rustc --release --features ffi --crate-type cdylib   # or: just build-ffi
# Linux:   target/release/libdependency_injector.so
# macOS:   target/release/libdependency_injector.dylib
# Windows: target/release/dependency_injector.dll
```

## FFI: running the binding test suites

`just test-bindings` builds the cdylib and runs all four suites. Full per-language docs
are in [ffi/README.md](ffi/README.md); the short version (Linux paths shown; on macOS use
`DYLD_LIBRARY_PATH` and `.dylib`):

- **Go** ([ffi/go/di](ffi/go/di)) — cgo link flags (`-L../../../target/release
  -ldependency_injector`) are baked into `di.go`; the dynamic loader still needs the path:
  ```bash
  cd ffi/go/di
  CGO_ENABLED=1 LD_LIBRARY_PATH=$PWD/../../../target/release go test -v ./...
  ```
- **Python** ([ffi/python](ffi/python)) — the loader honors `DI_LIBRARY_PATH` (a path to
  the library *file*) first, then falls back to the local `target/release/` build:
  ```bash
  cd ffi/python
  pip install -e ".[dev]"        # dev extra provides pytest
  DI_LIBRARY_PATH=$PWD/../../target/release/libdependency_injector.so python -m pytest
  ```
- **Node.js** ([ffi/nodejs](ffi/nodejs)) — `--ignore-scripts` skips the postinstall
  download of prebuilt binaries (you just built the library locally); the loader honors
  `DI_LIBRARY_PATH` first:
  ```bash
  cd ffi/nodejs
  pnpm install --ignore-scripts
  DI_LIBRARY_PATH=$PWD/../../target/release/libdependency_injector.so pnpm test
  ```
- **C#** ([ffi/csharp](ffi/csharp)) — the resolver checks `DI_LIBRARY_PATH`, then normal
  library search paths:
  ```bash
  cd ffi/csharp
  LD_LIBRARY_PATH=$PWD/../../target/release dotnet test
  ```

## Documentation site

The docs site is an Angular app in [docs/](docs/), deployed to GitHub Pages on merge to `main`:

```bash
cd docs
pnpm install
pnpm run build      # or from the repo root: just docs
```

## Benchmarks

```bash
cargo bench --bench container_bench     # internal benchmarks (just bench)
cargo bench --bench comparison_bench    # vs. other Rust DI crates
```

## Fuzzing

Requires nightly and `cargo-fuzz` (`cargo install cargo-fuzz`). Targets live in
[fuzz/fuzz_targets/](fuzz/fuzz_targets/): `fuzz_container`, `fuzz_scoped`,
`fuzz_concurrent`, `fuzz_lifecycle`.

```bash
cargo +nightly fuzz run fuzz_container            # or: just fuzz fuzz_container
cargo +nightly fuzz run fuzz_container -- -max_total_time=60
```

## Releases

Releases are tag-driven — see [RELEASING.md](RELEASING.md).

## Security

Please do not report security issues in public GitHub issues — see the
[Security Policy](.github/SECURITY.md).

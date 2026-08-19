# AGENTS.md — conventions for AI/CLI contributors

Load-bearing rules for working in this repo, consolidated from `.cursor/rules/*.mdc`
and the git hooks in `.cargo-husky/hooks/` (the hooks are the enforcement mechanism —
they install into `.git/hooks/` on the first `cargo test`).

## Project facts

- Cargo workspace with two crates: `dependency-injector` (root) and
  `dependency-injector-derive` ([dependency-injector-derive/](dependency-injector-derive/));
  versions kept in lockstep. The `fuzz/` crate is excluded from the workspace on purpose.
- **MSRV: Rust 1.85**, edition 2024. CI checks against 1.85.0 — do not use newer features.
- High-performance library: hot paths (`get()`, `contains()`, `try_get()`) target
  sub-10ns resolution. Use `#[inline]` on small hot methods, `Arc<T>` for sharing,
  `DashMap` (never `RwLock<HashMap>`). Every `unsafe` block needs a `// SAFETY:` comment.

## Commits (enforced by the commit-msg hook)

- Conventional Commits: `<type>(<scope>): <description>`
- Types: `feat` `fix` `docs` `style` `refactor` `perf` `test` `build` `ci` `chore` `revert`
- Subject ≤ **72 chars**, lowercase description (≥ 3 chars), **no trailing period**,
  blank line between subject and body.
- Scope is optional to the hook but repo convention is to include one:
  `container`, `storage`, `factory`, `macros`, `logging`, `bench`, `docs`, `ci`, `deps`.
- Breaking change: add `!` after the scope — `feat(container)!: rename get() to resolve()`.

## Lint gates (pre-commit hook + CI)

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
```

The pre-push hook escalates clippy to `-D clippy::all -D clippy::pedantic` with a
curated allow list — read [.cargo-husky/hooks/pre-push](.cargo-husky/hooks/pre-push)
for the exact `-A` flags before "fixing" a pedantic lint that is deliberately allowed.

## Testing expectations

- `cargo test --all-features` must pass (pre-push hook, CI, release gate).
- CI also runs the feature matrix — check these when touching feature-gated code:
  `cargo test --no-default-features`, `cargo test --features tracing`,
  `cargo test --features async`.
- Unit tests in `#[cfg(test)] mod tests` beside the code; integration tests in
  [tests/](tests/); Criterion benchmarks in [benches/](benches/).
- Fuzz targets in [fuzz/fuzz_targets/](fuzz/fuzz_targets/):
  `cargo +nightly fuzz run fuzz_container|fuzz_scoped|fuzz_concurrent|fuzz_lifecycle`.

## Documentation

- All public APIs need doc comments; complex APIs need `# Examples` with runnable code.
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features` must pass (pre-push + CI).

## Error handling

- Errors via `thiserror`; fallible operations return `Result<T, ContainerError>`.
- Provide `try_*` variants returning `Option<T>`.
- Errors carry context (e.g. `NotFound { type_name }`), never bare unit variants.

## FFI

Build the shared library with exactly:

```bash
cargo rustc --release --features ffi --crate-type cdylib
```

Binding suites and their env wiring: see [CONTRIBUTING.md](CONTRIBUTING.md) or run
`just test-bindings`.

## Versioning and releases

- Semantic versioning. **No breaking API changes without a major version bump**;
  mark them with `!` in the commit subject.
- Publish order when both crates change: `dependency-injector-derive` first, then
  `dependency-injector`.
- Releases are tag-driven — see [RELEASING.md](RELEASING.md). Do not run the legacy
  `scripts/deploy*.sh` to publish.

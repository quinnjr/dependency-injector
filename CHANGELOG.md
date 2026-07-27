# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **FFI verbs exposed in every language binding**: `remove`, `clear`, `lock`,
  and `is_locked`/`isLocked`/`IsLocked` are now bound in Python, Node.js, Go,
  and C#, together with the `LOCKED` (6) error code. Locking blocks
  registration only — removal, clearing, and resolution stay permitted —
  matching the core container, and child scopes start unlocked.
- **Correct `-1` sentinel handling across all bindings**: `contains` and
  `service_count` previously folded the native error sentinel into `false` and
  a count of `-1`. All four bindings now raise/throw instead, so an internal
  error can no longer masquerade as "service not registered" or a negative
  count. The C header's guidance was updated to match (it previously described
  the collapse as a known limitation).
- **Real async support**: `Container::lazy_async`, `get_async`, and
  `try_get_async` (feature `async`) — async-initialized singletons backed by
  `tokio::sync::OnceCell`. Successful initialization happens exactly once
  under concurrency (the factory itself may re-run if an in-flight attempt is
  cancelled or panics), and async registrations participate in scope and
  parent-chain resolution. The `async` feature previously compiled tokio in
  but gated no API.
- **New FFI verbs**: `di_remove`, `di_clear`, `di_lock`, `di_is_locked`, and
  a new `DI_LOCKED = 6` error code (append-only ABI). Locking blocks
  registration only, matching the core container's semantics.
- **Panic safety across the FFI boundary**: every `extern "C"` entry point now
  catches panics and surfaces them via `di_error_message` instead of unwinding
  into the host process (previously undefined behavior).
- `Container::debug_registrations()` — formats the scope chain's registration
  counts and `TypeId`s for diagnosing `NotFound` errors, which now carry an
  actionable hint.
- Derive macros accept `#[inject]` and `#[dep]` as exact aliases across
  `Inject`, `Service`, and `TypedRequire`.
- New examples: `scopes.rs`, `typed_builder.rs`, `performance.rs`; the docs
  site gained a Compile-Time Safety guide section.
- Release integrity: native release assets now ship with a `SHA256SUMS`
  file, and the Node/Python installers verify downloads against it.
- Staged downloads with atomic rename in both installers: the native library
  is written to a `<library>.download.<pid>` staging file next to its final path and
  is renamed into place only after checksum verification succeeds
  (`fs.renameSync` / `Path.replace`). The final library path therefore never
  holds an unverified, partially-written, or truncated file, and a failed
  verification deletes the staging file instead of leaving it behind.
- `DI_REQUIRE_CHECKSUM` strict mode in both installers: when set to a
  non-empty value, a release that carries no `SHA256SUMS` asset is a hard
  failure instead of a warn-and-continue (the default remains lenient so
  pre-checksum releases still install).
- Contributor tooling: `CONTRIBUTING.md`, `RELEASING.md`, `AGENTS.md`,
  `ROADMAP.md`, a `justfile` for the full dev loop, and a Cargo workspace
  (root + derive; fuzz stays standalone).
- CI: binding test suites (Python/Node/Go/C#) now run on every push; docs
  site builds on PRs; miri (informative), daily canary, nightly fuzz with
  corpus persistence, release verification, idempotent crates.io publishing,
  per-job timeouts, dependency audit + Dependabot coverage for all five
  package ecosystems.

### Changed
- **BREAKING (Go binding)**: `Contains` and `ServiceCount` now return
  `(bool, error)` and `(int64, error)` respectively. The native library
  returns `-1` from `di_contains`/`di_service_count` to signal an error, and
  the old single-value signatures collapsed that sentinel into `false` / a
  count of `-1`, so an internal failure was indistinguishable from "not
  registered" or reported as a negative service count. Callers must destructure
  the second value; `ffi/go/example/main.go` shows the updated form.
- **BREAKING (derive input)**: a field carrying more than one
  `#[inject]`/`#[dep]` marker is now a compile error spanning the duplicate
  attribute. Previously the first marker won and every later one was silently
  ignored — on *any* struct, not just structs with several derives on them,
  since duplicate helper attributes are inert to rustc. Code that relied on
  the silent first-wins behavior must drop the redundant markers.
- FFI locks now recover from poisoning instead of panicking: all eight
  `RwLock` acquisitions in `src/ffi.rs` use
  `unwrap_or_else(PoisonError::into_inner)` rather than `.unwrap()`. One
  caught panic no longer bricks a container for the remainder of its
  lifetime: previously every later call on that container hit the poisoned
  lock, panicked, and was surfaced as an error. Operations now proceed
  transparently against the (possibly partially-updated) state, so a poisoned
  lock produces neither a caught panic nor an error.
- FFI binding package versions (npm, PyPI, NuGet) synced from 0.2.2 to the
  crate's major version line.
- `DiError::NotFound`'s display message now explains likely causes and points
  at `debug_registrations()`.
- Note on derive helper-attribute aliasing: because `#[inject]` and `#[dep]`
  are now accepted interchangeably by `Inject`, `Service`, and `TypedRequire`,
  a field attribute named `#[dep]` or `#[inject]` coming from *another* derive
  ecosystem on the same struct could now be intercepted by these macros (rare
  in practice).
- Python installer (`ffi/python/scripts/download_native.py`) policy flip: a
  release with no matching native asset, or a download that fails or is
  truncated, now prints build instructions and exits 0 instead of exiting 1.
  Platform wheels bundle the library, so the sdist download path is a
  convenience rather than a hard requirement; this matches the Node
  installer. Checksum *failures* remain fatal (exit 1).
- `ffi/dependency_injector.h` now documents `-1` as an error sentinel for
  `di_contains()` and `di_is_locked()` — it means "invalid argument, consult
  `di_error_message()`", and callers must not collapse it into `false`. The
  header also documents the new FFI-boundary panic-safety contract.
- Published `.crate` contents shrank: `Cargo.toml`'s `exclude` list now also
  drops `AGENTS.md`, `RELEASING.md`, `ROADMAP.md`, and `justfile`.

### Fixed
- Node binding: `di_resolve_json`/`di_error_message` return values are now
  bound as raw pointers, and `di_string_free`'s *parameter* type changed to
  that same raw pointer type (which is what makes passing a JS string to it a
  type error). The previous koffi `char*` declaration auto-decoded the return
  value and then freed a koffi-owned temporary, corrupting the heap on every
  native-string crossing — every successful `resolve()` as well as every error
  message read, since `resolve()` routes its success path through the same
  `takeNativeString` helper (caught by the new bindings-test CI job).

### Security
- Node installer (`ffi/nodejs/scripts/install.js`): HTTP redirect following is
  now capped at 5 hops (previously uncapped, so a redirect loop never
  terminated and the install hung), and the `DI_GITHUB_TOKEN`
  `Authorization` header is attached only when the request host is
  `api.github.com`. The host check is re-evaluated at every redirect hop, so
  the token is no longer sent to download hosts (e.g.
  `objects.githubusercontent.com`) or to an attacker-chosen redirect target.

## [2.0.0] - 2026-07-21

### Added
- `Container::remove::<T>()` — removes a single service registration from the
  current scope (documented in the README since 1.0.0, now implemented)
- `typed::Require` trait — the `#[derive(TypedRequire)]` macro now generates
  valid code (it previously referenced non-existent items and failed to
  compile on any use)
- `Resolvable` is now implemented for tuples mixing required (`Arc<T>`) and
  optional (`Option<Arc<T>>`) dependencies, making `#[dep(optional)]` usable
  alongside other dependencies in `#[derive(Service)]`

### Fixed
- Thread-local hot cache is now automatically invalidated when a container is
  mutated (registration, `clear()`, `remove()`, batch registration, scope-pool
  release). Previously `clear()` or re-registration could serve stale cached
  services on the same thread.
- `ProviderRegistration::singleton(value)` now actually registers the value
  (was a silent no-op)
- Python and Node.js FFI bindings no longer leak the native string returned by
  every `resolve()` and error lookup. As part of this, an empty-string payload
  is now reported as a serialization error rather than "not found"
  (only a NULL pointer means not-found)
- FFI `di_register_singleton` duplicate check is now atomic (a concurrent
  duplicate registration can no longer silently overwrite)

### Changed
- **BREAKING**: `ProviderRegistration.register_fn` changed from
  `fn(&Container)` to `Arc<dyn Fn(&Container) + Send + Sync>` so registrations
  can capture the registered value. Code constructing `ProviderRegistration`
  literally or using the field as a plain `fn` pointer must be updated; the
  `provider!` macro and `(reg.register_fn)(&container)` call sites are
  unaffected. This is the change that makes this release 2.0.0.

## [0.2.2] - 2025-12-28

### Highlights
- **FFI Bindings** - Use dependency-injector from Go, Python, Node.js, and C#
- **Cross-Language Benchmarks** - Comprehensive comparison against 5 languages
- **Compile-Time Safety** - Type-state builder and verified service providers
- **Memory Verified** - Zero leaks confirmed by both dhat and Valgrind

### Added
- **FFI Support** - C-compatible bindings for cross-language integration
  - Go package with CGO bindings (`ffi/go/`)
  - Python package with ctypes (`ffi/python/`)
  - Node.js package with koffi (`ffi/nodejs/`) - no native compilation needed
  - C# library with P/Invoke (`ffi/csharp/`)
  - C header file (`ffi/dependency_injector.h`)
  - `di_resolve_json()` FFI function for JSON-based resolution
- **Compile-Time Safety** - New type-safe DI patterns
  - `TypedBuilder` / `TypedContainer` - Type-state builder pattern
  - `HasType<T>` trait for compile-time dependency verification
  - `Service` trait for declaring service dependencies
  - `ServiceProvider` trait for automatic registration
  - `Resolvable` trait for generic resolution (tuples, Option, etc.)
  - `ServiceModule` trait for grouping related services
- **Memory Profiling** - `memory_profiler` example with dhat integration
- **Deploy Scripts** - Automated release tooling
  - `scripts/deploy.sh` - Rust library deployment to crates.io
  - `scripts/deploy-ffi.sh` - FFI package deployment (npm, PyPI, NuGet)
- **Cursor Agents** - AI-assisted development workflows
  - `rust-di-expert` - DI pattern guidance
  - `performance-optimizer` - Benchmark analysis
  - `docs-writer` - Documentation generation
  - `test-engineer` - Test coverage
  - `release-manager` - Publishing workflow

### Changed
- Node.js FFI bindings now use `koffi` instead of `ffi-napi` (no native compilation)
- Node.js package requires `pnpm` as package manager (enforced via `only-allow`)
- `cdylib` crate type now conditional on `ffi` feature (use `cargo rustc --features ffi --crate-type cdylib`)
- Go FFI uses `di_resolve_json()` for cleaner JSON handling
- Improved `nil` safety in Go `Container.Free()` method

### Documentation
- **New FFI Bindings page** (`/docs/ffi`) with comprehensive language guides
- **SEO/AEO Enhancements**
  - JSON-LD structured data (SoftwareSourceCode, FAQPage, HowTo schemas)
  - Open Graph and Twitter Card meta tags
  - `sitemap.xml` and `robots.txt`
  - Per-page dynamic meta tags via `SeoService`
- Updated examples page with cross-language code snippets
- Updated getting-started with FFI links
- Added `BENCHMARK_COMPARISON.md` for 5-language comparison
- Added `RUST_DI_COMPARISON.md` for Rust ecosystem comparison

### Benchmarks
Cross-language comparison results:

| Language | Library | Singleton | Mixed Workload |
|----------|---------|-----------|----------------|
| **Rust** | dependency-injector | **17-32 ns** | **2.2 µs** |
| Go | samber/do | 767 ns | 125 µs |
| C# | MS.Extensions.DI | 208 ns | 31 µs |
| Python | dependency-injector | 95 ns | 15.7 µs |
| Node.js | inversify | 1,829 ns | 15 µs |

Rust DI comparison:

| Library | Singleton | Mixed Workload |
|---------|-----------|----------------|
| **dependency-injector** | **17-27 ns** | **2.2 µs** |
| shaku | 17-21 ns | 2.5-15 µs |
| ferrous-di | 57-70 ns | 7.6-11 µs |

### Memory Profiling
Verified with dhat and Valgrind:
- **Definitely lost: 0 bytes**
- **Indirectly lost: 0 bytes**
- **Possibly lost: 0 bytes**
- Total allocations: 51,808, properly freed: 51,804 (99.99%)

## [0.2.1] - 2025-12-21

### Highlights
- **~9.4ns singleton resolution** - now only 12% overhead vs manual DI
- **20% faster error paths** - root container fast-path optimization

### Changed
- Replaced `RefCell` with `UnsafeCell` in thread-local hot cache
- Store pre-computed `u64` type hash instead of `TypeId` (faster comparisons)
- Added `#[cold]` annotation to `resolve_from_parents` (better branch prediction)
- Fast path for root containers skips parent chain walk
- Added `#[inline(always)]` to critical hot cache methods

### Performance
| Operation | Before | After | Improvement |
|-----------|--------|-------|-------------|
| `get_singleton` | 9.8 ns | 9.4 ns | 4% faster |
| `try_get_not_found` | 13.7 ns | 10.9 ns | 20% faster |
| Gap to manual DI | 1.4 ns | 1.0 ns | 12% overhead |

## [0.2.0] - 2025-12-21

### Highlights
- **~9ns singleton resolution** - within 1ns of manual dependency injection
- **Full feature set** - scopes, pooling, derive macros, perfect hashing
- **Lock-free concurrency** - DashMap + thread-local cache

### Added
- `#[derive(Inject)]` macro for compile-time dependency injection
- `ScopePool` for pre-allocated scope reuse in high-throughput scenarios
- `FrozenStorage` with perfect hashing for static containers
- Thread-local hot cache for frequently accessed services
- Fluent batch registration API (`container.register_batch().singleton(A).done()`)
- Deep parent chain resolution for multi-level hierarchies
- `perfect-hash` feature flag for frozen container support
- `logging`, `logging-json`, and `logging-pretty` feature flags

### Changed
- Replaced `RwLock<bool>` with `AtomicBool` for lock state
- Switched to enum-based `AnyFactory` (eliminated vtable indirection)
- Reduced DashMap shards for child scopes (8 → 4)
- Optimized hot cache with fast bit-mixing hash

### Performance
| Operation | Time |
|-----------|------|
| `get_singleton` | ~9 ns |
| `get_transient` | ~24 ns |
| `create_scope` | ~80 ns |
| `scope_pool_acquire` | ~56 ns |
| `frozen_contains` | ~4 ns |

## [0.1.12] - 2025-12-21

### Changed
- Fast bit-mixing hash in hot cache (golden ratio multiplication)
- Single DashMap lookup via `get_with_transient_flag()`
- Reduced shard count for child scopes (8 → 4)

### Performance
- All resolution benchmarks now under 10ns for cached services
- `get_singleton`: 14.7ns → 9ns (40% faster)
- `get_transient`: 43ns → 24ns (44% faster)

## [0.1.11] - 2025-12-20

### Added
- `perfect-hash` feature with `FrozenStorage` using MPHF
- `container.freeze()` method for immutable containers

### Performance
- `frozen_contains`: 3.9ns (60% faster than DashMap)

## [0.1.10] - 2025-12-20

### Added
- Deep parent chain resolution for grandparent and beyond

### Changed
- `ServiceStorage` now holds optional parent reference for chain walking

## [0.1.9] - 2025-12-19

### Changed
- Unsafe unchecked downcast for Arc (TypeId already verified)

### Performance
- ~5-7% faster resolution across all benchmarks

## [0.1.8] - 2025-12-19

### Added
- Fluent batch registration API: `container.batch().singleton(A).done()`

### Performance
- Batch registration ~1% faster than individual registrations

## [0.1.7] - 2025-12-18

### Added
- `ScopePool` for pre-allocated scope reuse
- `PooledScope` RAII guard for automatic release

### Performance
- 30% faster scope acquisition vs fresh creation

## [0.1.6] - 2025-12-18

### Added
- Thread-local hot cache for frequently accessed services
- `clear_cache()` and `warm_cache<T>()` methods

### Performance
- 21% faster singleton resolution (18.7ns → 14.8ns)
- 48% faster parent resolution (28.7ns → 14.8ns)

## [0.1.5] - 2025-12-17

### Added
- `#[derive(Inject)]` compile-time DI macro
- `#[inject]` and `#[inject(optional)]` attributes
- `from_container()` method generation

## [0.1.4] - 2025-12-17

### Added
- Batch registration API with `BatchRegistrar`

## [0.1.3] - 2025-12-16

### Changed
- Enum-based `AnyFactory` (eliminated vtable indirection)
- Pre-erased `Arc<dyn Any>` storage in factories
- Cached parent `Arc<ServiceStorage>`

## [0.1.2] - 2025-12-16

### Changed
- Replaced `RwLock<bool>` with `AtomicBool` for lock state
- Optimized DashMap shard count (8 shards default)
- Removed `parking_lot` dependency

### Performance
- Registration: 854ns → 250ns (71% faster)

## [0.1.1] - 2025-12-15

### Added
- Initial release with core DI functionality
- Singleton, lazy, and transient lifetimes
- Scoped containers with parent resolution
- Lock-free concurrent access via DashMap

[0.2.2]: https://github.com/pegasusheavy/dependency-injector/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/pegasusheavy/dependency-injector/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.12...v0.2.0
[0.1.12]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.11...v0.1.12
[0.1.11]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/pegasusheavy/dependency-injector/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/pegasusheavy/dependency-injector/releases/tag/v0.1.1

# Roadmap

Larger initiatives identified by the 2026-07-22 suggestion audit. Each entry
carries the evidence that motivated it and a design sketch. Items small enough
to land directly were implemented in the same audit branch; these were not,
because each needs a dedicated design/review cycle.

## 1. Multi-lifetime support across the FFI (transient / lazy factories)

**Evidence**: `ffi/dependency_injector.h` exposes only
`di_register_singleton(_json)`, while the core's headline is
singleton/lazy/transient (`src/container.rs`). Every binding (Python, Node,
Go, C#) is capped at singletons.

**Sketch**: a factory-callback ABI —
`di_register_transient(container, name, factory_fn, user_data, destructor_fn)`
where `factory_fn: extern "C" fn(user_data: *mut c_void) -> DiOwnedBytes`.
Each binding marshals its native closures (ctypes `CFUNCTYPE`, koffi
callbacks, cgo `//export` trampolines, C# delegates + `GCHandle`). Open
questions: callback thread-safety contract (resolve can happen on any
thread), reentrancy (factory calling back into the container), and lifetime
of `user_data` (needs the destructor). Ship behind new ABI symbols only —
no changes to existing exports.

## 2. Back the FFI store with a shared core substrate

**Evidence**: `src/ffi.rs` reimplements storage as
`RwLock<HashMap<String, Arc<dyn Any>>>` with snapshot scopes, while
`src/storage.rs` already solves live parent chains + generation-stamped
invalidation. The 2.0.0 invalidation work never reached the FFI because the
implementations are disjoint.

**Sketch**: generalize `ServiceStorage` over its key
(`ServiceStorage<K: Eq + Hash>`; `K = TypeId` core, `K = String` FFI), or
extract the parent-chain + generation mechanics into a shared internal
`ChainStore<K>`. FFI scopes become live instead of snapshots (a documented
behavior change for bindings — release-note it). Lifetime parity stays in
item 1; this item is storage semantics only.

## 3. Converge the dependency-declaration traits (breaking → 3.0)

**Evidence**: three encodings of "this service needs X": `typed::Require`
(type-level, consumed by nothing at runtime), `typed::DeclaresDeps`
(`&'static [&'static str]`), `verified::DependencyInfo` (`Vec<&'static str>`),
emitted by three derives that each re-parse the same fields.

**Sketch**: make `Service::Dependencies` canonical; provide the string views
via blanket projections. Blanket `impl<T: Service> Require for T` conflicts
with derived `Require` impls (coherence), so this is a breaking consolidation:
deprecate `Require`/`DeclaresDeps` in 2.x, remove in 3.0.

## 4. One scope concept (breaking → 3.0)

**Evidence**: `Container::scope()`, `ScopedContainer` (pure delegation plus a
`Scope` id), and `ScopePool` overlap. `ScopedContainer` *is* used —
`examples/scopes.rs`, `examples/logging.rs`, and the docs site's API page
(`docs/src/app/pages/docs/api/api.html`) all reference it — but only as a
pass-through for what `Container` already does. `ScopeBuilder` is exercised
only by its own doctest (`src/scope.rs:255-273`) and the `test_scope_builder`
unit test (`src/scope.rs:386`) — both compiled and executed, but no consumer
in `examples/`, `tests/`, or the docs site references it. The case for
consolidation is the redundant API surface, not disuse.

**Sketch**: fold the `Scope` id into `Container` (it already tracks `depth`),
reduce `ScopedContainer` to a deprecated alias in 2.x, remove in 3.0. Keep
`ScopePool` — pooling is the one genuinely distinct concept.

## 5. Python distribution via maturin

**Evidence**: `build-python-wheels.yml` builds a generic wheel with raw
setuptools, hand-renames it to a platform tag with `sed`, and bundles the
cdylib — while `download_native.py` still exists for the sdist path.

**Sketch**: switch `ffi/python` to maturin (`bindings = "cffi"`-less pure
`cdylib` bundling), which emits correctly-tagged manylinux/macos/windows
wheels natively; retire the rename hack and reduce `download_native.py` to
the sdist fallback only. NuGet's `runtimes/{rid}/native` layout in
`ffi/csharp` is the model. Requires validating the wheel matrix end to end
before switching PyPI publishing.

## 6. Shared ABI conformance suite + binding codegen

**Evidence**: four bindings hand-restate the C header (ctypes argtypes,
koffi `lib.func`, P/Invoke, cgo) and each maintains a private test suite with
no shared vectors — semantic drift between bindings is only caught by
accident.

**Sketch**: one `ffi/conformance/vectors.json` (sequences of
register/resolve/scope/contains/remove ops with expected codes/values); each
binding's test harness gains a replay runner. Phase two: generate the
mechanical declaration blocks from `dependency_injector.h` (cbindgen
round-trip or a small script).

## 7. Release automation via release-plz

**Evidence**: three parallel release implementations (deploy.sh's sed
version bumps and `sleep 30` index waits, publish.sh's curl-and-grep
version check, release.yml's bare `cargo publish`) plus two changelog
generators (deploy.sh `awk`, release.yml `grep`). Both had bugs found in
production this week (dry-run mutation; non-idempotent publish).

**Sketch**: release-plz release-PR flow driving the existing tag-triggered
workflows; git-cliff (built in) replaces both changelog generators; the
local scripts retire. `RELEASING.md` documents the current canonical flow
until this lands.

- npm publish is not tag-triggered today: `scripts/deploy-ffi.sh` is the only
  path that runs `pnpm publish` for `@pegasusheavy/dependency-injector`. Fold
  a tag-driven npm publish into this release-plz work so all binding packages
  ship from CI.

## 8. Remaining hardening follow-ups

- TSAN job (needs `-Z build-std`; miri landed first).
- Tighten the miri CI job once miri-clean. Baseline:
  `cargo +nightly miri test --lib` currently reports ~95 errors
  (predominantly memory-leak reports from thread-local hot-cache state and
  FFI test allocations); re-run to re-baseline before tightening the CI job.
- Configure `deny.toml` and drop the `|| true` on cargo-deny in
  `audit.yml` (currently masks all failures).
- Consider `try_lazy_async` returning `Result` for fallible async
  initialization (the current factory can only fail by panicking).
- Seed a `fuzz_lifecycle` corpus (currently cold-starts every run).
- Generate the six per-language `DiErrorCode` enums from the header
  (the `From<DiError>` mapping landed; codegen is phase two).
- Surface `Locked`/`CircularDependency` as first-class ABI codes in the
  next ABI-extending release.
- Add a `trybuild` dev-dependency to `dependency-injector-derive` for
  compile-fail coverage. Nothing currently asserts on a derive *rejection*:
  the new duplicate-`#[inject]`/`#[dep]` marker diagnostic is untested, and so
  is every `to_compile_error()` arm in `Inject`, `Service`, and `TypedRequire`
  — non-struct input, a struct with unnamed fields, and the field-type
  mismatches (`#[inject]` on a non-`Arc<T>` field; `#[inject(optional)]` on a
  non-`Option<Arc<T>>` field). `trybuild` would be dev-only on the derive
  crate, so it does not touch the published dependency graph.
- Propagate the `-1` error sentinel through the four language bindings.
  `di_contains()` returns `-1` for an invalid argument (null container, null
  or non-UTF-8 type name), but Python, Node, Go, and C# all test `result == 1`
  and so collapse `-1` into `false` — callers cannot distinguish "not
  registered" from "bad argument" without consulting `di_error_message()`.
  The header documents this as a known limitation; the fix is per-binding
  (raise/throw, or return a tri-state). `di_is_locked()` has the same
  sentinel and is not yet bound anywhere, so it should be wired up correctly
  from the start — as should the rest of the new verbs (`di_remove`,
  `di_clear`, `di_lock`), none of which any binding exposes today.
- Decide what to do about the inert CI cache keys. `Cargo.lock` is gitignored
  repo-wide and no lockfile is tracked, so every
  `hashFiles('**/Cargo.lock')` component (`ci.yml` check/clippy/test/features/
  doc/msrv/bindings, `bench.yml`) and every `hashFiles('fuzz/Cargo.lock')`
  component (`ci.yml`, `nightly-fuzz.yml`) matches no files and expands to the
  empty string — the keys never vary, so the caches never invalidate on a
  dependency change. Either commit lockfiles for CI-key purposes (the usual
  library-crate tradeoff) or drop the fiction and key on something that
  actually changes.

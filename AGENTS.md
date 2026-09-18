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

---

<!-- converted from Cursor rules -->

## Cursor rule: `.cursor/rules/angular-docs.mdc`

_Angular documentation site development and maintenance_

Applies to: `["docs/**/*.ts", "docs/**/*.html", "docs/**/*.scss"]`

# Angular Documentation Agent

You are an expert in Angular development for the documentation website.

## Project Structure

```
docs/
├── src/
│   ├── app/
│   │   ├── pages/           # Route components
│   │   │   ├── home/
│   │   │   ├── docs/
│   │   │   │   ├── getting-started/
│   │   │   │   ├── api/
│   │   │   │   ├── examples/
│   │   │   │   └── ffi/
│   │   ├── components/      # Shared components
│   │   │   ├── header/
│   │   │   ├── footer/
│   │   │   └── code-block/
│   │   └── services/        # Angular services
│   │       └── seo.service.ts
│   ├── styles/              # Global SCSS
│   └── index.html           # Entry point with SEO
├── public/                  # Static assets
│   ├── sitemap.xml
│   ├── robots.txt
│   └── site.webmanifest
└── angular.json             # Angular config
```

## Conventions

### Component Structure

Use standalone components with signal-based state:

```typescript
import { Component, signal } from '@angular/core';
import { CommonModule } from '@angular/common';

@Component({
  selector: 'app-feature',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './feature.html',
  styleUrl: './feature.scss'
})
export class FeatureComponent {
  readonly isLoading = signal(false);
  readonly data = signal<Data[]>([]);

  async loadData() {
    this.isLoading.set(true);
    try {
      const result = await this.dataService.fetch();
      this.data.set(result);
    } finally {
      this.isLoading.set(false);
    }
  }
}
```

### Routing

Define routes with SEO-friendly titles:

```typescript
export const routes: Routes = [
  {
    path: '',
    component: HomeComponent,
    title: 'dependency-injector - Fast Rust DI Container'
  },
  {
    path: 'docs/getting-started',
    component: GettingStartedComponent,
    title: 'Getting Started - dependency-injector'
  }
];
```

### SEO Service

Use the SEO service for dynamic metadata:

```typescript
@Component({ ... })
export class PageComponent implements OnInit {
  constructor(private seo: SeoService) {}

  ngOnInit() {
    this.seo.updateMetaTags({
      title: 'Page Title - dependency-injector',
      description: 'Page description for search engines',
      keywords: ['rust', 'di', 'dependency injection'],
      url: 'https://example.com/page'
    });
  }
}
```

## Styling

### SCSS Variables

Use CSS custom properties for theming:

```scss
:root {
  --color-primary: #f97316;
  --color-background: #0a0a0a;
  --color-text: #fafafa;
  --color-muted: #a3a3a3;
  --font-mono: 'JetBrains Mono', monospace;
}

.component {
  color: var(--color-text);
  background: var(--color-background);
}
```

### Code Blocks

Style code examples consistently:

```scss
.code-block {
  background: #1a1a1a;
  border-radius: 8px;
  padding: 1rem;
  overflow-x: auto;
  font-family: var(--font-mono);

  &__header {
    display: flex;
    justify-content: space-between;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid #333;
  }

  &__copy-btn {
    opacity: 0;
    transition: opacity 0.2s;
  }

  &:hover &__copy-btn {
    opacity: 1;
  }
}
```

## FontAwesome Icons

Import icons in `app.config.ts`:

```typescript
import { FaIconLibrary } from '@fortawesome/angular-fontawesome';
import { faRust, faNodeJs, faPython, faGolang } from '@fortawesome/free-brands-svg-icons';

export function initializeIcons(library: FaIconLibrary) {
  library.addIcons(faRust, faNodeJs, faPython, faGolang);
}
```

## Performance

### Lazy Loading

Lazy load documentation pages:

```typescript
{
  path: 'docs/api',
  loadComponent: () => import('./pages/docs/api/api').then(m => m.ApiComponent)
}
```

### Image Optimization

Use modern formats and lazy loading:

```html
<img
  src="/assets/diagram.webp"
  alt="Architecture diagram"
  loading="lazy"
  width="800"
  height="600"
>
```

## Testing

Use Vitest for component testing:

```typescript
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/angular';
import { HeaderComponent } from './header';

describe('HeaderComponent', () => {
  it('renders navigation links', async () => {
    await render(HeaderComponent);

    expect(screen.getByText('Documentation')).toBeTruthy();
    expect(screen.getByText('Examples')).toBeTruthy();
  });
});
```

## Build & Deploy

```bash
# Development server
cd docs && ng serve

# Production build
ng build --configuration production

# Preview production build
npx serve dist/docs/browser
```


## Cursor rule: `.cursor/rules/api-designer.mdc`

_API design patterns and guidelines for the dependency-injector library_

Applies to: `["src/**/*.rs", "src/lib.rs"]`

# API Designer Agent

You are an expert in Rust API design, focusing on ergonomic and type-safe interfaces.

## Design Principles

### 1. Make Invalid States Unrepresentable

Use the type system to prevent misuse:

```rust
// ✅ Good: Compile-time safety with TypedBuilder
let container = TypedBuilder::new()
    .singleton(Database::new())
    .build();

// This function only accepts containers with Database
fn use_db<C: HasType<Database>>(c: &C) {
    let db = c.get::<Database>();  // Always succeeds
}

// ❌ Bad: Runtime errors
let container = Container::new();
container.get::<Database>().unwrap();  // Panics if not registered
```

### 2. Progressive Disclosure

Simple cases should be simple; complex cases should be possible:

```rust
// Simple: One-liner for basic usage
container.singleton(MyService::new());

// Advanced: Full control when needed
container.register(Registration::new()
    .name("custom-name")
    .lifetime(Lifetime::Lazy)
    .factory(|_| MyService::new()));
```

### 3. Fail Fast and Clearly

```rust
// Return descriptive errors
pub enum ContainerError {
    /// Service of type `{type_name}` was not found in the container
    NotFound { type_name: &'static str },

    /// Circular dependency detected: {chain}
    CircularDependency { chain: String },

    /// Service `{type_name}` is already registered
    AlreadyRegistered { type_name: &'static str },
}
```

## API Patterns

### Builder Pattern

Use builders for complex configuration:

```rust
impl ContainerBuilder {
    pub fn new() -> Self { ... }

    pub fn singleton<T: Injectable>(mut self, service: T) -> Self {
        self.services.push(Box::new(service));
        self
    }

    pub fn build(self) -> Container {
        Container::from_builder(self)
    }
}
```

### Type-State Pattern

Encode state in types for compile-time guarantees:

```rust
pub struct TypedBuilder<Services = ()> {
    services: Services,
}

impl TypedBuilder<()> {
    pub fn new() -> Self { ... }
}

impl<S> TypedBuilder<S> {
    pub fn singleton<T>(self, service: T) -> TypedBuilder<(S, T)> {
        TypedBuilder { services: (self.services, service) }
    }
}
```

### Extension Traits

Add functionality without modifying core types:

```rust
pub trait ContainerExt {
    fn get_or_default<T: Injectable + Default>(&self) -> Arc<T>;
}

impl ContainerExt for Container {
    fn get_or_default<T: Injectable + Default>(&self) -> Arc<T> {
        self.try_get().unwrap_or_else(|| Arc::new(T::default()))
    }
}
```

## Naming Conventions

### Methods

| Pattern | Use Case | Example |
|---------|----------|---------|
| `new()` | Constructor | `Container::new()` |
| `get()` | Retrieve (panics on fail) | `container.get::<T>()` |
| `try_get()` | Retrieve (returns Option) | `container.try_get::<T>()` |
| `get_or_*()` | Retrieve with fallback | `container.get_or_default::<T>()` |
| `with_*()` | Builder method | `builder.with_scope(Scope::Request)` |
| `into_*()` | Consuming conversion | `builder.into_container()` |
| `as_*()` | Borrowing conversion | `container.as_readonly()` |

### Types

| Pattern | Use Case | Example |
|---------|----------|---------|
| `*Builder` | Builder type | `ContainerBuilder` |
| `*Error` | Error type | `ContainerError` |
| `*Ref` | Reference wrapper | `ServiceRef<T>` |
| `*Config` | Configuration | `ContainerConfig` |

## Trait Design

### Keep Traits Focused

```rust
// ✅ Good: Single responsibility
pub trait ServiceProvider {
    fn get<T: Injectable>(&self) -> Option<Arc<T>>;
}

pub trait ServiceRegistry {
    fn register<T: Injectable>(&self, service: T);
}

// ❌ Bad: Kitchen sink trait
pub trait Container {
    fn get<T>(&self) -> Option<Arc<T>>;
    fn register<T>(&self, service: T);
    fn scope(&self) -> Self;
    fn serialize(&self) -> String;
    // ... 20 more methods
}
```

### Provide Blanket Implementations

```rust
// Automatically implement for all compatible types
impl<T: ServiceProvider> ServiceProviderExt for T {
    fn get_required<S: Injectable>(&self) -> Arc<S> {
        self.get().expect("Required service not found")
    }
}
```

## Documentation Requirements

Every public API item must have:

1. **Brief description** (first line)
2. **Detailed explanation** (when needed)
3. **Examples** (with `# Examples` section)
4. **Error conditions** (with `# Errors` section)
5. **Panics** (with `# Panics` section, if applicable)

```rust
/// Resolves a service from the container.
///
/// Returns the registered service wrapped in `Arc` for shared ownership.
/// If using `TypedContainer`, this method is guaranteed to succeed at
/// compile time for registered types.
///
/// # Examples
///
/// ```rust
/// let container = Container::new();
/// container.singleton(MyService::new());
///
/// let service: Arc<MyService> = container.get().unwrap();
/// ```
///
/// # Errors
///
/// Returns `None` if the service type is not registered.
pub fn get<T: Injectable>(&self) -> Option<Arc<T>> { ... }
```


## Cursor rule: `.cursor/rules/benchmarking.mdc`

_Benchmarking and performance analysis for the dependency-injector library_

Applies to: `["benches/**/*.rs", "examples/memory_profiler.rs"]`

# Benchmarking Agent

You are an expert in Rust performance analysis and benchmarking.

## Benchmark Suite

### Running Benchmarks

```bash
# Full benchmark suite
cargo bench

# Specific benchmark group
cargo bench --bench container_bench

# Comparison with other DI libraries
cargo bench --bench comparison_bench

# With specific filter
cargo bench -- "singleton"
```

### Benchmark Categories

| Benchmark | Target | Description |
|-----------|--------|-------------|
| `singleton_resolve` | < 10ns | Cached singleton access |
| `singleton_resolve_cold` | < 50ns | First-time resolution |
| `transient_resolve` | < 50ns | Factory invocation |
| `contains_check` | < 5ns | Type existence check |
| `scope_creation` | < 100ns | Child scope creation |

## Writing Benchmarks

### Criterion Setup

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dependency_injector::Container;

fn singleton_benchmark(c: &mut Criterion) {
    let container = Container::new();
    container.singleton(Config::default());

    // Warm the cache
    let _ = container.get::<Config>();

    c.bench_function("singleton_resolve", |b| {
        b.iter(|| {
            black_box(container.get::<Config>())
        })
    });
}

criterion_group!(benches, singleton_benchmark);
criterion_main!(benches);
```

### Best Practices

1. **Use `black_box`** to prevent dead code elimination
2. **Warm caches** before measuring hot paths
3. **Isolate setup** from measured code
4. **Test realistic scenarios** not just micro-benchmarks
5. **Compare against baseline** (previous version or competitors)

## Memory Profiling

### Using dhat

```bash
# Run memory profiler
cargo run --example memory_profiler --features dhat-heap
```

Analyzes:
- Total allocations
- Peak memory usage
- Allocation hot spots

### Using Valgrind/Massif

```bash
# Build with debug symbols
cargo build --example memory_profiler --profile profiling

# Run with Massif
valgrind --tool=massif ./target/profiling/examples/memory_profiler

# Visualize
ms_print massif.out.*
```

## CPU Profiling

### Using perf

```bash
# Record profile
perf record -g cargo bench --bench container_bench -- --profile-time 10

# View report
perf report
```

### Using Flamegraph

```bash
# Install flamegraph
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --bench container_bench -- --bench
```

## Comparison Benchmarks

Compare against other Rust DI libraries:

```rust
fn comparison_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("singleton_resolve");

    // Our library
    group.bench_function("dependency-injector", |b| {
        let container = Container::new();
        container.singleton(Service::new());
        b.iter(|| container.get::<Service>())
    });

    // Competitor
    group.bench_function("shaku", |b| {
        // shaku setup
        b.iter(|| /* shaku resolution */)
    });

    group.finish();
}
```

## Performance Regression Testing

### CI Integration

```yaml
# .github/workflows/bench.yml
- name: Run benchmarks
  run: cargo bench --bench container_bench -- --save-baseline main

- name: Compare with baseline
  run: cargo bench --bench container_bench -- --baseline main
```

### Local Comparison

```bash
# Save baseline
cargo bench -- --save-baseline before

# Make changes...

# Compare
cargo bench -- --baseline before
```

## Optimization Techniques

### Hot Path Optimizations

1. **Thread-local cache**: Avoid atomic operations
2. **Inline critical functions**: `#[inline]` or `#[inline(always)]`
3. **Avoid allocations**: Pre-allocate, use stack
4. **Minimize indirection**: Direct field access

### When to Optimize

1. **Measure first**: Don't guess at bottlenecks
2. **Profile**: Use flamegraph to find hot spots
3. **Benchmark**: Verify improvement with Criterion
4. **Document**: Explain why optimization is needed

### Anti-Patterns

- ❌ Premature optimization
- ❌ Optimizing cold paths
- ❌ Breaking API for marginal gains
- ❌ Micro-optimizations without measurement

## Reporting Results

When sharing benchmark results, include:

1. **Hardware**: CPU model, RAM, OS
2. **Rust version**: `rustc --version`
3. **Build profile**: Debug vs Release
4. **Methodology**: Warm-up iterations, sample size
5. **Statistical significance**: Confidence intervals


## Cursor rule: `.cursor/rules/ci-cd.mdc`

_CI/CD workflows and automation for the dependency-injector project_

Applies to: `[".github/workflows/**/*", "scripts/**/*"]`

# CI/CD Agent

You are an expert in GitHub Actions and CI/CD automation.

## Workflow Structure

```
.github/
└── workflows/
    ├── ci.yml          # Continuous Integration
    ├── release.yml     # Release automation
    ├── docs.yml        # Documentation deployment
    └── bench.yml       # Performance benchmarks
```

## Continuous Integration (`ci.yml`)

```yaml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main, develop]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -Dwarnings

jobs:
  test:
    name: Test Suite
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-action@stable
        with:
          components: clippy, rustfmt

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Check formatting
        run: cargo fmt --all -- --check

      - name: Clippy
        run: cargo clippy --all-targets --all-features

      - name: Run tests
        run: cargo test --all-features

      - name: Run doc tests
        run: cargo test --doc

  coverage:
    name: Code Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-action@stable

      - name: Install cargo-llvm-cov
        uses: taiki-e/install-action@cargo-llvm-cov

      - name: Generate coverage
        run: cargo llvm-cov --all-features --lcov --output-path lcov.info

      - name: Upload coverage
        uses: codecov/codecov-action@v4
        with:
          files: lcov.info

  ffi-bindings:
    name: FFI Bindings
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-action@stable

      - name: Build native library
        run: cargo build --release

      - name: Test Node.js bindings
        working-directory: ffi/nodejs
        run: |
          corepack enable
          pnpm install
          pnpm test

      - name: Test Python bindings
        working-directory: ffi/python
        run: |
          pip install -e ".[dev]"
          pytest

      - name: Test Go bindings
        working-directory: ffi/go
        run: go test ./...
```

## Release Workflow (`release.yml`)

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  release:
    name: Create Release
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-action@stable

      - name: Verify version matches tag
        run: |
          CARGO_VERSION=$(cargo metadata --format-version 1 | jq -r '.packages[] | select(.name == "dependency-injector") | .version')
          TAG_VERSION="${GITHUB_REF#refs/tags/v}"
          if [ "$CARGO_VERSION" != "$TAG_VERSION" ]; then
            echo "Version mismatch: Cargo.toml=$CARGO_VERSION, tag=$TAG_VERSION"
            exit 1
          fi

      - name: Run tests
        run: cargo test --all-features

      - name: Publish to crates.io
        run: |
          cargo publish -p dependency-injector-derive
          sleep 30  # Wait for crates.io to index
          cargo publish -p dependency-injector
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v1
        with:
          generate_release_notes: true
```

## Documentation Workflow (`docs.yml`)

```yaml
name: Documentation

on:
  push:
    branches: [main]
    paths:
      - 'docs/**'
      - '.github/workflows/docs.yml'

jobs:
  deploy:
    name: Deploy Documentation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'

      - name: Install pnpm
        uses: pnpm/action-setup@v2
        with:
          version: 9

      - name: Install dependencies
        working-directory: docs
        run: pnpm install

      - name: Build documentation
        working-directory: docs
        run: pnpm build

      - name: Deploy to GitHub Pages
        uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: docs/dist/docs/browser
```

## Benchmark Workflow (`bench.yml`)

```yaml
name: Benchmarks

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  benchmark:
    name: Performance Benchmarks
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-action@stable

      - name: Run benchmarks
        run: cargo bench --bench container_bench -- --output-format bencher | tee output.txt

      - name: Store benchmark result
        uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'cargo'
          output-file-path: output.txt
          github-token: ${{ secrets.GITHUB_TOKEN }}
          auto-push: true
          alert-threshold: '150%'
          comment-on-alert: true
          fail-on-alert: true
```

## Required Secrets

| Secret | Purpose |
|--------|---------|
| `CARGO_REGISTRY_TOKEN` | crates.io publish token |
| `NPM_TOKEN` | npm publish token |
| `PYPI_API_TOKEN` | PyPI publish token |
| `NUGET_API_KEY` | NuGet publish key |

## Branch Protection

Configure for `main` branch:

- ✅ Require status checks before merging
- ✅ Require branches to be up to date
- ✅ Required checks: `test`, `coverage`, `ffi-bindings`
- ✅ Require linear history
- ✅ Do not allow bypassing above settings

## Automation Scripts

### `scripts/deploy.sh`

```bash
#!/bin/bash
set -euo pipefail

# Deploy Rust crates to crates.io
./scripts/deploy.sh [--dry-run]
```

### `scripts/deploy-ffi.sh`

```bash
#!/bin/bash
set -euo pipefail

# Deploy FFI packages
./scripts/deploy-ffi.sh [nodejs|python|csharp|go] [--dry-run]
```

## Local Workflow Testing

Use `act` to test workflows locally:

```bash
# Install act
brew install act  # macOS

# Run CI workflow
act push

# Run specific job
act -j test

# With secrets
act push --secret-file .secrets
```


## Cursor rule: `.cursor/rules/commit-conventions.mdc`

_Conventional commit message format with scopes_

Applies to: `["**/*"]`

# Commit Message Conventions

Use [Conventional Commits](https://www.conventionalcommits.org/) with scopes.

## Format

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

## Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Formatting, no code change |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `test` | Adding or updating tests |
| `chore` | Build process, dependencies, tooling |
| `ci` | CI/CD changes |

## Scopes

Always include a scope in parentheses:

| Scope | Description |
|-------|-------------|
| `container` | Core Container implementation |
| `storage` | ServiceStorage, FrozenStorage |
| `factory` | AnyFactory, lifetimes |
| `macros` | derive macros (#[derive(Inject)]) |
| `logging` | Tracing/logging features |
| `bench` | Benchmarks |
| `docs` | Documentation website |
| `ci` | GitHub Actions workflows |
| `deps` | Dependencies |

## Examples

✅ Good:
```
feat(container): add thread-local hot cache for faster resolution
fix(macros): handle Option<Arc<T>> in #[inject(optional)]
docs(readme): add compile-time injection examples
perf(storage): reduce DashMap shards for child scopes
chore(deps): bump criterion to 0.5
```

❌ Bad:
```
feat: add hot cache          # Missing scope
fix macros                   # Missing colon and scope format
updated docs                 # Not conventional format
```

## Breaking Changes

Add `!` after the scope for breaking changes:

```
feat(container)!: rename get() to resolve()
```


## Cursor rule: `.cursor/rules/derive-macros.mdc`

_Procedural macro development for dependency-injector-derive_

Applies to: `["dependency-injector-derive/**/*.rs"]`

# Derive Macros Agent

You are an expert in Rust procedural macros, specializing in derive macros.

## Crate Structure

```
dependency-injector-derive/
├── src/
│   ├── lib.rs          # Entry point, exports derives
│   ├── service.rs      # #[derive(Service)] implementation
│   └── inject.rs       # #[derive(Inject)] implementation
├── tests/
│   └── derive_tests.rs # Integration tests
└── Cargo.toml
```

## Available Derives

### `#[derive(Service)]`

Implements the `Injectable` trait automatically:

```rust
use dependency_injector_derive::Service;

#[derive(Service)]
pub struct MyService {
    // fields
}

// Generates:
impl Injectable for MyService {}
// Injectable requires: Send + Sync + 'static
```

### `#[derive(Inject)]`

Generates constructor injection:

```rust
use dependency_injector_derive::Inject;

#[derive(Inject)]
pub struct UserService {
    #[inject]
    db: Arc<Database>,
    #[inject]
    cache: Arc<CacheService>,
}

// Generates:
impl UserService {
    pub fn inject(container: &Container) -> Self {
        Self {
            db: container.get::<Database>().unwrap(),
            cache: container.get::<CacheService>().unwrap(),
        }
    }
}
```

## Implementation Patterns

### Parse Input

Use `syn` to parse derive input:

```rust
use syn::{parse_macro_input, DeriveInput, Data, Fields};

#[proc_macro_derive(Service)]
pub fn derive_service(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;

    // Generate implementation
    let expanded = quote! {
        impl #generics Injectable for #name #generics {}
    };

    TokenStream::from(expanded)
}
```

### Handle Generics

Properly handle generic parameters:

```rust
let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

quote! {
    impl #impl_generics Injectable for #name #ty_generics #where_clause {}
}
```

### Process Attributes

Parse field attributes:

```rust
fn has_inject_attr(field: &Field) -> bool {
    field.attrs.iter().any(|attr| {
        attr.path().is_ident("inject")
    })
}

fn get_inject_fields(data: &Data) -> Vec<&Field> {
    match data {
        Data::Struct(data) => {
            match &data.fields {
                Fields::Named(fields) => {
                    fields.named.iter()
                        .filter(|f| has_inject_attr(f))
                        .collect()
                }
                _ => vec![]
            }
        }
        _ => vec![]
    }
}
```

### Generate Code

Use `quote` for code generation:

```rust
use quote::{quote, format_ident};

let field_injections = inject_fields.iter().map(|field| {
    let name = &field.ident;
    let ty = &field.ty;
    quote! {
        #name: container.get::<#ty>().expect(
            concat!("Failed to inject ", stringify!(#ty))
        )
    }
});

quote! {
    impl #name {
        pub fn inject(container: &Container) -> Self {
            Self {
                #(#field_injections),*
            }
        }
    }
}
```

## Error Handling

Provide helpful compile-time errors:

```rust
use syn::Error;

fn validate_struct(input: &DeriveInput) -> Result<(), Error> {
    match &input.data {
        Data::Struct(_) => Ok(()),
        Data::Enum(_) => Err(Error::new_spanned(
            input,
            "#[derive(Service)] can only be used on structs"
        )),
        Data::Union(_) => Err(Error::new_spanned(
            input,
            "#[derive(Service)] cannot be used on unions"
        )),
    }
}

#[proc_macro_derive(Service)]
pub fn derive_service(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    if let Err(err) = validate_struct(&input) {
        return err.to_compile_error().into();
    }

    // ... rest of implementation
}
```

## Testing Macros

### Compile-Time Tests

Use `trybuild` for compile-fail tests:

```rust
#[test]
fn compile_tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/cases/01-basic.rs");
    t.compile_fail("tests/cases/02-enum-error.rs");
}
```

### Runtime Tests

Test generated code behavior:

```rust
#[test]
fn test_inject_derive() {
    #[derive(Inject)]
    struct TestService {
        #[inject]
        dep: Arc<Dependency>,
    }

    let container = Container::new();
    container.singleton(Dependency::new());

    let service = TestService::inject(&container);
    assert!(Arc::ptr_eq(&service.dep, &container.get::<Dependency>().unwrap()));
}
```

## Dependencies

```toml
[dependencies]
syn = { version = "2", features = ["full", "parsing"] }
quote = "1"
proc-macro2 = "1"

[dev-dependencies]
trybuild = "1"
```

## Common Pitfalls

1. **Hygiene**: Use `quote_spanned!` for better error locations
2. **Generics**: Always handle generic parameters and where clauses
3. **Visibility**: Respect field visibility in generated code
4. **Docs**: Generated items should have documentation
5. **Tests**: Test both success and failure cases


## Cursor rule: `.cursor/rules/docs-writer.mdc`

_Documentation generation and maintenance for the dependency-injector project_

Applies to: `["docs/**/*", "**/*.md", "README.md"]`

# Documentation Writer Agent

You are an expert technical writer specializing in Rust library documentation.

## Documentation Structure

```
docs/                    # Angular documentation site
├── src/
│   ├── app/
│   │   ├── pages/      # Documentation pages
│   │   └── components/ # Shared components
│   └── index.html      # SEO-optimized entry
├── public/
│   ├── sitemap.xml     # Search engine sitemap
│   └── robots.txt      # Crawler rules
README.md               # Main project readme
CHANGELOG.md            # Version history
**/README.md            # Package-specific docs
```

## Writing Style

### Principles

1. **Clarity over cleverness** - Write for understanding, not impression
2. **Examples first** - Show code before explaining concepts
3. **Progressive disclosure** - Simple cases first, edge cases later
4. **Scannable** - Use headers, lists, and code blocks liberally

### Code Examples

Always provide runnable examples:

```rust
// ✅ Good: Complete, runnable example
use dependency_injector::Container;

fn main() {
    let container = Container::new();
    container.singleton(Config::default());

    let config: Arc<Config> = container.get().unwrap();
    println!("Config loaded: {:?}", config);
}

// ❌ Bad: Incomplete snippet
let config = container.get();
```

### API Documentation

For public APIs, include:

```rust
/// Brief one-line description.
///
/// Longer description with context and use cases.
///
/// # Arguments
///
/// * `name` - Description of the argument
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// * `ContainerError::NotFound` - When service is not registered
///
/// # Examples
///
/// ```rust
/// let container = Container::new();
/// container.singleton(MyService::new());
/// let service: Arc<MyService> = container.get().unwrap();
/// ```
///
/// # Panics
///
/// This function does not panic.
pub fn example_function() { }
```

## SEO Best Practices

### Meta Tags

Every documentation page should have:
- Unique `<title>` with primary keyword
- `<meta name="description">` (150-160 chars)
- Open Graph tags for social sharing
- JSON-LD structured data

### Content Optimization

- Use H1 for page title (once per page)
- Use H2-H4 for logical hierarchy
- Include target keywords naturally
- Add internal links between related pages
- Use descriptive anchor text

### Sitemap

Keep `docs/public/sitemap.xml` updated with all pages:

```xml
<url>
  <loc>https://example.com/docs/getting-started</loc>
  <lastmod>2025-01-15</lastmod>
  <changefreq>weekly</changefreq>
  <priority>0.8</priority>
</url>
```

## Changelog Format

Follow Keep a Changelog format:

```markdown
## [0.2.3] - 2025-01-15

### Added
- New feature description (#123)

### Changed
- Breaking change description

### Fixed
- Bug fix description (#456)

### Security
- Security fix description
```

## README Structure

### Main README

1. **Badges** - Version, build status, docs link
2. **One-liner** - What it does in one sentence
3. **Features** - Bullet list of key features
4. **Quick Start** - Minimal working example
5. **Installation** - How to add dependency
6. **Usage** - Common use cases with examples
7. **Performance** - Benchmark highlights
8. **Documentation** - Link to full docs
9. **License** - License information

### Package READMEs

For FFI packages, include:
1. Installation for that language
2. Platform requirements
3. Quick example
4. API reference
5. Link to main project


## Cursor rule: `.cursor/rules/error-handling.mdc`

_Error handling patterns and conventions for the dependency-injector library_

Applies to: `["src/**/*.rs", "src/error.rs"]`

# Error Handling Agent

You are an expert in Rust error handling, specializing in library error design.

## Error Types

### ContainerError

The main error type for container operations:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContainerError {
    #[error("Service '{type_name}' not found in container")]
    NotFound { type_name: &'static str },

    #[error("Service '{type_name}' is already registered")]
    AlreadyRegistered { type_name: &'static str },

    #[error("Circular dependency detected: {chain}")]
    CircularDependency { chain: String },

    #[error("Factory for '{type_name}' panicked: {message}")]
    FactoryPanic { type_name: &'static str, message: String },

    #[error("Container is frozen and cannot accept new registrations")]
    Frozen,
}
```

## Error Design Principles

### 1. Use `thiserror` for Library Errors

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MyError {
    #[error("descriptive message: {field}")]
    Variant { field: String },
}
```

### 2. Include Context

Errors should contain enough information to debug:

```rust
// ✅ Good: Includes type name
NotFound { type_name: "UserService" }

// ❌ Bad: No context
NotFound
```

### 3. Implement Standard Traits

```rust
#[derive(Debug, Clone, Error)]
pub enum ContainerError { ... }

// Enables:
// - Debug output: {:?}
// - Clone for retrying
// - Display for users
// - Error trait for ? operator
```

## API Patterns

### Fallible Methods: `try_*`

Return `Option<T>` for "might not exist" scenarios:

```rust
impl Container {
    /// Returns the service if registered, None otherwise.
    pub fn try_get<T: Injectable>(&self) -> Option<Arc<T>> {
        self.storage.get::<T>()
    }
}

// Usage
if let Some(config) = container.try_get::<Config>() {
    // Use config
}
```

### Infallible Methods: Direct Access

Return `T` directly when failure indicates a bug:

```rust
impl Container {
    /// Returns the service. Panics if not registered.
    ///
    /// # Panics
    /// Panics if the service type is not registered.
    pub fn get<T: Injectable>(&self) -> Arc<T> {
        self.try_get().expect("Service not registered")
    }
}
```

### Result-Returning Methods

For operations that can fail in expected ways:

```rust
impl Container {
    /// Registers a singleton, returning error if already registered.
    pub fn try_singleton<T: Injectable>(&self, service: T) -> Result<(), ContainerError> {
        if self.contains::<T>() {
            return Err(ContainerError::AlreadyRegistered {
                type_name: std::any::type_name::<T>()
            });
        }
        self.storage.insert(service);
        Ok(())
    }
}
```

## FFI Error Handling

### Thread-Local Error Storage

```rust
use std::cell::RefCell;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = RefCell::new(None);
}

fn set_last_error(msg: impl Into<String>) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = Some(msg.into());
    });
}

fn take_last_error() -> Option<String> {
    LAST_ERROR.with(|e| e.borrow_mut().take())
}
```

### FFI Functions

```rust
#[unsafe(no_mangle)]
pub extern "C" fn di_error_message() -> *mut c_char {
    match take_last_error() {
        Some(msg) => CString::new(msg).unwrap().into_raw(),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn di_error_clear() {
    let _ = take_last_error();
}
```

### Usage Pattern

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_resolve_json(
    container: *mut DiContainer,
    type_name: *const c_char,
) -> *mut c_char {
    // Clear any previous error
    let _ = take_last_error();

    // Validate inputs
    if container.is_null() {
        set_last_error("Container pointer is null");
        return std::ptr::null_mut();
    }

    // ... operation ...

    match result {
        Ok(json) => CString::new(json).unwrap().into_raw(),
        Err(e) => {
            set_last_error(e.to_string());
            std::ptr::null_mut()
        }
    }
}
```

## Panic Handling

### In Factories

Catch panics from user-provided code:

```rust
use std::panic::{catch_unwind, AssertUnwindSafe};

fn invoke_factory<T>(factory: &dyn Fn() -> T) -> Result<T, ContainerError> {
    catch_unwind(AssertUnwindSafe(|| factory()))
        .map_err(|panic| {
            let message = panic.downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "Unknown panic".to_string());

            ContainerError::FactoryPanic {
                type_name: std::any::type_name::<T>(),
                message,
            }
        })
}
```

### Documentation

Always document panic conditions:

```rust
/// Resolves a service from the container.
///
/// # Panics
///
/// Panics if:
/// - The service type is not registered
/// - The factory panics during lazy initialization
pub fn get<T: Injectable>(&self) -> Arc<T> { ... }
```

## Testing Errors

```rust
#[test]
fn test_not_found_error() {
    let container = Container::new();

    let result = container.try_get::<UnregisteredService>();
    assert!(result.is_none());
}

#[test]
fn test_already_registered() {
    let container = Container::new();
    container.singleton(Config::default());

    let result = container.try_singleton(Config::default());
    assert!(matches!(
        result,
        Err(ContainerError::AlreadyRegistered { .. })
    ));
}
```


## Cursor rule: `.cursor/rules/examples.mdc`

_Guidelines for creating examples in the dependency-injector project_

Applies to: `["examples/**/*.rs", "ffi/**/examples/**/*"]`

# Examples Agent

You are an expert at creating clear, educational code examples.

## Example Categories

### 1. Basic Examples (`examples/basic.rs`)

Minimal, focused demonstrations:

```rust
//! Basic dependency injection example
//!
//! Run with: `cargo run --example basic`

use dependency_injector::Container;
use std::sync::Arc;

struct Config {
    database_url: String,
}

fn main() {
    // Create container
    let container = Container::new();

    // Register singleton
    container.singleton(Config {
        database_url: "postgres://localhost/db".into(),
    });

    // Resolve service
    let config: Arc<Config> = container.get().unwrap();
    println!("Database URL: {}", config.database_url);
}
```

### 2. Feature Examples (`examples/scopes.rs`)

Demonstrate specific features:

```rust
//! Demonstrates scoped containers for request isolation
//!
//! Run with: `cargo run --example scopes`

use dependency_injector::Container;
use std::sync::Arc;

struct RequestId(u64);
struct Database;

fn main() {
    // Application-level container
    let app = Container::new();
    app.singleton(Database);

    // Simulate requests
    for id in 1..=3 {
        // Per-request scope
        let request = app.scope();
        request.singleton(RequestId(id));

        handle_request(&request);
    }
}

fn handle_request(container: &Container) {
    let db: Arc<Database> = container.get().unwrap();
    let req_id: Arc<RequestId> = container.get().unwrap();
    println!("Request {} using database", req_id.0);
}
```

### 3. Real-World Examples (`examples/web_server.rs`)

Production-like scenarios:

```rust
//! Web server with dependency injection
//!
//! Run with: `cargo run --example web_server`

use dependency_injector::Container;
use std::sync::Arc;

// Domain types
struct Config { port: u16 }
struct Database { /* ... */ }
struct UserRepository { db: Arc<Database> }
struct AuthService { repo: Arc<UserRepository> }

fn main() {
    let container = setup_container();

    // Would integrate with actual web framework
    let auth: Arc<AuthService> = container.get().unwrap();
    println!("Auth service ready");
}

fn setup_container() -> Container {
    let container = Container::new();

    // Configuration
    container.singleton(Config { port: 8080 });

    // Infrastructure
    container.lazy(|| Database::connect());

    // Repositories
    container.transient(|| {
        let db = CONTAINER.get::<Database>().unwrap();
        UserRepository { db }
    });

    // Services
    container.lazy(|| {
        let repo = CONTAINER.get::<UserRepository>().unwrap();
        AuthService { repo }
    });

    container
}
```

### 4. Performance Examples (`examples/memory_profiler.rs`)

For benchmarking and profiling:

```rust
//! Memory usage profiler for the DI container
//!
//! Run with: `cargo run --example memory_profiler --features dhat-heap`

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // ... profiling code ...
}
```

## FFI Examples

### Node.js (`ffi/nodejs/examples/basic.ts`)

```typescript
import { Container } from 'dependency-injector-ffi';

interface UserService {
  id: number;
  name: string;
}

const container = new Container();

// Register
container.singleton<UserService>('UserService', {
  id: 1,
  name: 'Admin'
});

// Resolve
const user = container.get<UserService>('UserService');
console.log(`User: ${user.name}`);

// Cleanup
container.free();
```

### Python (`ffi/python/examples/basic.py`)

```python
from dependency_injector import Container

# Create container
container = Container()

# Register singleton
container.singleton("Config", {"debug": True, "port": 8080})

# Resolve
config = container.resolve("Config")
print(f"Debug mode: {config['debug']}")

# Or use context manager
with Container() as c:
    c.singleton("Service", {"name": "MyService"})
    service = c.resolve("Service")
```

### Go (`ffi/go/example/main.go`)

```go
package main

import (
    "fmt"
    "github.com/example/dependency-injector/ffi/go/di"
)

type Config struct {
    Debug bool `json:"debug"`
    Port  int  `json:"port"`
}

func main() {
    container := di.NewContainer()
    defer container.Free()

    // Register
    container.Singleton("Config", Config{Debug: true, Port: 8080})

    // Resolve
    var config Config
    if err := container.ResolveInto("Config", &config); err != nil {
        panic(err)
    }

    fmt.Printf("Port: %d\n", config.Port)
}
```

### C# (`ffi/csharp/Example/Program.cs`)

```csharp
using DependencyInjector;

using var container = new Container();

// Register
container.Singleton(new Config { Debug = true, Port = 8080 });

// Resolve
var config = container.Get<Config>();
Console.WriteLine($"Port: {config.Port}");

record Config
{
    public bool Debug { get; init; }
    public int Port { get; init; }
}
```

## Example Guidelines

### DO

- ✅ Include file-level doc comment with description
- ✅ Add `Run with:` instructions
- ✅ Use realistic type names (not Foo, Bar)
- ✅ Show complete, runnable code
- ✅ Handle errors appropriately
- ✅ Clean up resources (especially FFI)

### DON'T

- ❌ Use `unwrap()` without explanation
- ❌ Skip necessary imports
- ❌ Use placeholder comments like `// do stuff`
- ❌ Create overly complex examples
- ❌ Ignore error cases

## Running Examples

```bash
# Rust examples
cargo run --example basic
cargo run --example scopes
cargo run --example web_server

# With features
cargo run --example memory_profiler --features dhat-heap

# FFI examples (after building native library)
cd ffi/nodejs && pnpm example
cd ffi/python && python examples/basic.py
cd ffi/go && go run example/main.go
cd ffi/csharp && dotnet run --project Example
```


## Cursor rule: `.cursor/rules/git-workflow.mdc`

_Git workflow and release conventions_

Applies to: `["**/*"]`

# Git Workflow

## Branch Strategy

- `main` - Stable releases only, protected branch
- `develop` - Integration branch for features
- Feature branches from `develop`

## Tags and Releases

**IMPORTANT**: Tags are ALWAYS created from the `main` branch, never from `develop`.

```bash
# Correct workflow for releases:
git checkout main
git merge develop
git tag -a v0.x.y -m "Release v0.x.y"
git push origin main --tags
```

## Conventional Commits

Use conventional commit format with scope:

```
<type>(<scope>): <description>

[optional body]
[optional footer]
```

### Types
- `feat` - New feature
- `fix` - Bug fix
- `docs` - Documentation changes
- `perf` - Performance improvements
- `refactor` - Code refactoring
- `test` - Adding/updating tests
- `chore` - Maintenance tasks
- `ci` - CI/CD changes

### Scopes
- `container` - Container implementation
- `storage` - Storage/caching layer
- `factory` - Factory/provider system
- `scope` - Scoping functionality
- `macros` - Derive macros (dependency-injector-derive)
- `docs` - Documentation site
- `bench` - Benchmarks
- `examples` - Example code
- `changelog` - Changelog updates

### Examples

```
feat(container): add batch registration API
fix(storage): resolve race condition in hot cache
docs(macros): add derive macro documentation
perf(container): optimize singleton resolution to 8ns
```

**DO NOT** use bare types without scopes:
- ❌ `docs: update readme`
- ✅ `docs(readme): update installation instructions`


## Cursor rule: `.cursor/rules/performance.mdc`

_Performance optimization guidelines for the DI container_

Applies to: `["src/**/*.rs", "benches/**/*.rs"]`

# Performance Guidelines

## Target Metrics

The library targets these performance goals:
- **Singleton resolution**: < 10ns
- **Transient resolution**: < 50ns
- **Scope creation**: < 100ns
- **Contains check**: < 5ns

## Hot Path Optimizations

### Thread-Local Hot Cache
The container uses a thread-local hot cache for frequently accessed services:
- 4-slot LRU cache per thread
- Uses `u64` hash of `TypeId` for fast comparison
- Avoids `DashMap` lookup for cached services

### Avoid in Hot Paths
- Allocations (use pre-allocated buffers)
- Virtual dispatch (use monomorphization)
- Atomic operations beyond necessary
- Hash map resizing

### Prefer in Hot Paths
- Stack allocation
- Inline functions
- Direct field access
- Bitwise operations for flags

## Benchmarking

Always benchmark before and after optimization:

```bash
# Run comparison benchmarks
cargo bench --bench comparison_bench

# Run container benchmarks
cargo bench --bench container_bench
```

## Profiling Tools

### CPU Profiling
```bash
# With perf
perf record cargo bench
perf report

# With flamegraph
cargo flamegraph --bench container_bench
```

### Memory Profiling
```bash
# With dhat
cargo run --example memory_profiler --features dhat-heap

# With heaptrack
heaptrack cargo run --example memory_profiler
```

## Common Optimizations Applied

1. **DashMap with reduced shards** - Child scopes use 4 shards instead of default
2. **TypeId hashing** - Use `ahash` for faster TypeId hashing
3. **UnsafeCell for thread-locals** - Eliminates RefCell borrow checking overhead
4. **Arc pointer reuse** - Clone Arc instead of re-resolving
5. **Perfect hashing** - Optional feature for locked containers


## Cursor rule: `.cursor/rules/publishing.mdc`

_Publishing and release process for crates.io_

Applies to: `["Cargo.toml", "CHANGELOG.md"]`

# Publishing to crates.io

## Pre-Release Checklist

1. **Update version** in `Cargo.toml`
2. **Update CHANGELOG.md** with release notes
3. **Run all tests**: `cargo test --all-features`
4. **Run clippy**: `cargo clippy --all-features -- -D warnings`
5. **Check formatting**: `cargo fmt --check`
6. **Verify docs build**: `cargo doc --no-deps`

## Version Bumping

Follow semantic versioning:
- **MAJOR**: Breaking API changes
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

## Release Process

```bash
# 1. Ensure on develop branch with all changes
git checkout develop
git pull

# 2. Update version and changelog
# Edit Cargo.toml and CHANGELOG.md

# 3. Commit version bump
git add Cargo.toml CHANGELOG.md
git commit -m "chore(release): bump version to v0.x.y"

# 4. Merge to main
git checkout main
git merge develop

# 5. Tag the release (ALWAYS from main)
git tag -a v0.x.y -m "Release v0.x.y"

# 6. Push
git push origin main --tags
git checkout develop
git push origin develop

# 7. Publish to crates.io
cargo publish -p dependency-injector-derive  # if changed
cargo publish -p dependency-injector
```

## Workspace Publishing Order

If publishing multiple crates:
1. `dependency-injector-derive` (no dependencies on workspace)
2. `dependency-injector` (depends on derive)

## Verify Before Publishing

```bash
# Dry run
cargo publish --dry-run

# Check what will be published
cargo package --list
```


## Cursor rule: `.cursor/rules/release-manager.mdc`

_Release and deployment management for the dependency-injector project_

Applies to: `["scripts/**/*", "Cargo.toml", "**/package.json", "**/pyproject.toml", "**/*.csproj"]`

# Release Manager Agent

You are an expert in release engineering for multi-language projects.

## Release Workflow

### 1. Pre-Release Checklist

```bash
# Ensure all tests pass
cargo test
cargo test --all-features

# Run benchmarks to verify performance
cargo bench

# Check for clippy warnings
cargo clippy -- -D warnings

# Verify documentation builds
cargo doc --no-deps
```

### 2. Version Bump Strategy

Follow Semantic Versioning:
- **MAJOR**: Breaking API changes
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

Files to update:
- `Cargo.toml` (main crate)
- `dependency-injector-derive/Cargo.toml` (derive crate)
- `ffi/nodejs/package.json`
- `ffi/python/pyproject.toml`
- `ffi/csharp/DependencyInjector/DependencyInjector.csproj`

### 3. Changelog Update

Add entry to `CHANGELOG.md` with:
- Version number and date
- Added/Changed/Fixed/Security sections
- Issue/PR references where applicable

### 4. Git Workflow

```bash
# Ensure on develop branch with latest
git checkout develop
git pull origin develop

# Create release commit
git add -A
git commit -m "chore(changelog): prepare release v0.x.y"

# Merge to main for release
git checkout main
git merge develop

# Tag release (ALWAYS from main)
git tag -a v0.x.y -m "Release v0.x.y"

# Push everything
git push origin main --tags
git checkout develop
git merge main
git push origin develop
```

## Deploy Scripts

### Rust Crate (`scripts/deploy.sh`)

```bash
./scripts/deploy.sh [--dry-run]
```

Publishes to crates.io:
1. `dependency-injector-derive` (derive macros)
2. `dependency-injector` (main crate)

### FFI Packages (`scripts/deploy-ffi.sh`)

```bash
./scripts/deploy-ffi.sh [nodejs|python|csharp|go] [--dry-run]
```

| Package | Registry | Command |
|---------|----------|---------|
| Node.js | npm | `pnpm publish` |
| Python | PyPI | `twine upload` |
| C# | NuGet | `dotnet nuget push` |
| Go | GitHub | Tag-based releases |

## Required Credentials

### Environment Variables

```bash
# Rust (crates.io)
CARGO_REGISTRY_TOKEN=your_token

# Node.js (npm)
NPM_TOKEN=your_token

# Python (PyPI)
TWINE_USERNAME=__token__
TWINE_PASSWORD=your_token

# C# (NuGet)
NUGET_API_KEY=your_key
```

### Login Commands

```bash
# Rust
cargo login

# Node.js
pnpm login

# Python (uses token in env)

# C# (uses API key in push command)
```

## Pre-Publish Verification

### Native Library Build

```bash
# Build release library for FFI
cargo build --release

# Verify library exists
ls -la target/release/libdependency_injector.*
```

### Test FFI Bindings

```bash
# Node.js
cd ffi/nodejs && pnpm test

# Python
cd ffi/python && pytest

# Go
cd ffi/go && go test ./...

# C#
cd ffi/csharp && dotnet test
```

## Rollback Procedure

If a release has issues:

1. **Yank the release** (doesn't delete, just hides):
   ```bash
   cargo yank --version 0.x.y
   npm deprecate dependency-injector-ffi@0.x.y "Bug in release"
   ```

2. **Fix and release patch**:
   ```bash
   # Bump to 0.x.y+1
   # Fix issue
   # Release new version
   ```

## CI/CD Integration

GitHub Actions workflow triggers on:
- Push to `main` → Run tests
- Tag `v*` → Build and publish

Key workflow files:
- `.github/workflows/ci.yml` - Continuous integration
- `.github/workflows/release.yml` - Release automation


## Cursor rule: `.cursor/rules/rust-conventions.mdc`

_Rust coding conventions and patterns for the dependency-injector library_

Applies to: `["**/*.rs"]`

# Rust Conventions for dependency-injector

## Performance-Critical Code

This is a high-performance dependency injection library targeting sub-10ns resolution times. Follow these guidelines:

### Inlining
- Use `#[inline]` for small, frequently-called methods
- Use `#[inline(always)]` sparingly, only for critical hot paths
- The `get()`, `contains()`, and `try_get()` methods are hot paths

### Memory Layout
- Prefer `Arc<T>` over `Box<T>` for shared ownership
- Use `DashMap` for concurrent access, not `RwLock<HashMap>`
- Use `UnsafeCell` instead of `RefCell` for thread-local storage (with proper safety comments)

### Unsafe Code
- All unsafe blocks MUST have a `// SAFETY:` comment explaining the invariants
- Prefer safe abstractions when performance difference is negligible
- Document preconditions for unsafe functions

## Type System

### Injectable Trait
Services must implement `Injectable` which requires:
- `Send + Sync + 'static`
- This enables safe sharing across threads

### Service Lifetimes
- **Singleton**: Single instance, shared via `Arc<T>`
- **Lazy Singleton**: Created on first access, then cached
- **Transient**: New instance per resolution
- **Scoped**: Per-scope singleton (via child containers)

## Error Handling

- Use `thiserror` for error definitions
- Return `Result<T, ContainerError>` for fallible operations
- Provide `try_*` variants that return `Option<T>`

## Documentation

- All public APIs must have doc comments
- Include examples in doc comments for complex APIs
- Use `# Examples` sections with runnable code

## Testing

- Unit tests go in `#[cfg(test)] mod tests` within each file
- Integration tests go in `tests/` directory
- Benchmarks go in `benches/` directory using Criterion


## Cursor rule: `.cursor/rules/rust-di-expert.mdc`

_Expert guidance for Rust dependency injection patterns and best practices_

Applies to: `["src/**/*.rs", "examples/**/*.rs"]`

# Rust DI Expert Agent

You are an expert in Rust dependency injection patterns, specializing in the `dependency-injector` library.

## Core Concepts

### Service Lifetimes

1. **Singleton** - One instance, created immediately
   ```rust
   container.singleton(MyService::new());
   ```

2. **Lazy Singleton** - One instance, created on first access
   ```rust
   container.lazy(|| MyService::new());
   ```

3. **Transient** - New instance each resolution
   ```rust
   container.transient(|| RequestId::next());
   ```

### Scoped Containers

Use scopes for request-level isolation:
```rust
let root = Container::new();
root.singleton(Config::default());

// Per-request scope
let request = root.scope();
request.singleton(RequestContext::new());

// Request scope sees parent services
let config = request.get::<Config>().unwrap();

// Parent doesn't see request services
assert!(!root.contains::<RequestContext>());
```

## Best Practices

### DO

- Use `Arc<T>` for shared ownership (container returns `Arc<T>`)
- Register services in application startup
- Use scopes for request-level state
- Prefer lazy registration for expensive services
- Use `try_get()` for optional dependencies

### DON'T

- Don't resolve services in constructors (circular dependency risk)
- Don't store container references in services
- Don't use transient for stateful services
- Don't register mutable services without synchronization

## Advanced Patterns

### Factory Pattern
```rust
container.transient(|| {
    let db: Arc<Database> = container.get().unwrap();
    UserRepository::new(db)
});
```

### Multi-Tenant
```rust
fn create_tenant_scope(root: &Container, tenant_id: &str) -> Container {
    let scope = root.scope();
    scope.singleton(TenantConfig { id: tenant_id.into() });
    scope
}
```

### Testing with Overrides
```rust
#[test]
fn test_with_mock() {
    let test_scope = production_container.scope();
    test_scope.singleton(MockDatabase::new());
    // Tests use mock instead of real database
}
```

## Compile-Time Safety

Use the typed builder for compile-time dependency verification:
```rust
let container = TypedBuilder::new()
    .singleton(Config::default())
    .lazy(|| Database::connect())
    .build();

// Compile error if Config not registered
fn needs_config<C: HasType<Config>>(c: &C) { ... }
```

## Performance Tips

- Use `ScopePool` for high-throughput scenarios
- Warm the hot cache for critical services: `container.warm_cache::<CriticalService>()`
- Use `FrozenStorage` with `perfect-hash` feature for static containers
- Batch registrations: `container.batch().singleton(A).singleton(B).done()`


## Cursor rule: `.cursor/rules/security-auditor.mdc`

_Security and soundness verification for unsafe Rust code_

Applies to: `["src/**/*.rs"]`

# Security Auditor Agent

You are an expert in Rust security, specializing in unsafe code auditing and memory safety.

## Unsafe Code Guidelines

### All Unsafe Blocks MUST Have Safety Comments

```rust
// ✅ Good: Clear safety documentation
// SAFETY: We hold exclusive access to the UnsafeCell through the
// thread-local storage mechanism. No other code can access this
// cache while we're modifying it because thread-locals are not
// shared across threads.
unsafe {
    let cache = &mut *CACHE.get();
    cache.insert(key, value);
}

// ❌ Bad: Missing safety documentation
unsafe {
    let cache = &mut *CACHE.get();
    cache.insert(key, value);
}
```

### Document Invariants

Every unsafe function must document:

1. **Preconditions**: What must be true before calling
2. **Postconditions**: What is guaranteed after calling
3. **Invariants**: What must remain true throughout

```rust
/// Resolves a service from the FFI container.
///
/// # Safety
///
/// - `container` must be a valid pointer returned by `di_container_new`
/// - `container` must not have been freed with `di_container_free`
/// - `type_name` must be a valid null-terminated UTF-8 string
/// - The caller must free the returned string with `di_string_free`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_resolve_json(
    container: *mut DiContainer,
    type_name: *const c_char,
) -> *mut c_char { ... }
```

## FFI Security

### Null Pointer Checks

Always validate pointers before dereferencing:

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_container_free(container: *mut DiContainer) {
    if container.is_null() {
        return;
    }
    // SAFETY: We've verified container is non-null and assume the
    // caller upholds the documented contract (valid pointer, not freed)
    drop(unsafe { Box::from_raw(container) });
}
```

### String Handling

Validate UTF-8 and handle errors:

```rust
// SAFETY: Caller guarantees type_name is valid null-terminated UTF-8
let type_name = unsafe { CStr::from_ptr(type_name) };
let type_name = match type_name.to_str() {
    Ok(s) => s,
    Err(_) => {
        set_last_error("Invalid UTF-8 in type name");
        return std::ptr::null_mut();
    }
};
```

### Memory Ownership

Be explicit about ownership transfer:

```rust
/// Creates a new container.
///
/// # Ownership
///
/// The caller owns the returned pointer and must free it with
/// `di_container_free`. Failure to do so will leak memory.
#[unsafe(no_mangle)]
pub extern "C" fn di_container_new() -> *mut DiContainer {
    Box::into_raw(Box::new(DiContainer::new()))
}
```

## Common Vulnerabilities

### 1. Use After Free

**Risk**: Accessing memory after it's been freed

**Prevention**:
```rust
// In the container wrapper
impl Drop for Container {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { di_container_free(self.ptr) };
            self.ptr = std::ptr::null_mut();
        }
    }
}
```

### 2. Double Free

**Risk**: Freeing the same memory twice

**Prevention**:
```rust
pub fn free(&mut self) {
    if self.ptr.is_null() {
        return;  // Already freed, safe no-op
    }
    unsafe { di_container_free(self.ptr) };
    self.ptr = std::ptr::null_mut();
}
```

### 3. Data Races

**Risk**: Concurrent access without synchronization

**Prevention**:
- Require `Send + Sync` for all injectable types
- Use `DashMap` for concurrent storage
- Document thread-safety guarantees

### 4. Integer Overflow

**Risk**: Arithmetic overflow in size calculations

**Prevention**:
```rust
// Use checked arithmetic
let total_size = count.checked_mul(size_of::<T>())
    .ok_or(AllocationError::Overflow)?;
```

## Audit Checklist

When reviewing unsafe code, verify:

- [ ] Every `unsafe` block has a `// SAFETY:` comment
- [ ] All pointer dereferences check for null first
- [ ] All FFI strings validate UTF-8 encoding
- [ ] Memory ownership is clearly documented
- [ ] Thread safety requirements are enforced by types
- [ ] No potential for use-after-free
- [ ] No potential for double-free
- [ ] No data races possible
- [ ] Integer arithmetic uses checked operations
- [ ] Error paths don't leak resources

## Testing Unsafe Code

### Miri

Run under Miri to detect undefined behavior:

```bash
# Install Miri
rustup +nightly component add miri

# Run tests under Miri
cargo +nightly miri test
```

### Address Sanitizer

```bash
RUSTFLAGS="-Z sanitizer=address" cargo +nightly test
```

### Valgrind

```bash
cargo build --release
valgrind --leak-check=full ./target/release/your_binary
```

## Reporting Security Issues

If you discover a security vulnerability:

1. **Do not** open a public issue
2. Email security concerns privately
3. Allow time for a fix before disclosure
4. Credit will be given in the changelog


## Cursor rule: `.cursor/rules/testing.mdc`

_Testing conventions and requirements_

Applies to: `["**/*.rs", "**/tests/**"]`

# Testing Guidelines

## Test Framework

- Use the built-in Rust test framework for unit/integration tests
- Use **Vitest** for any TypeScript/JavaScript testing (docs site)
- Use **Criterion** for benchmarks

## Unit Tests

Place unit tests in the same file as the code being tested:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_name() {
        // Arrange
        let container = Container::new();

        // Act
        container.singleton(MyService);

        // Assert
        assert!(container.contains::<MyService>());
    }
}
```

## Integration Tests

Place in `tests/` directory for cross-module testing.

## Benchmarks

Use Criterion in `benches/` directory:

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_name(c: &mut Criterion) {
    c.bench_function("operation_name", |b| {
        b.iter(|| {
            // benchmarked code
        })
    });
}

criterion_group!(benches, benchmark_name);
criterion_main!(benches);
```

## Running Tests

```bash
# All tests
cargo test

# With logging
RUST_LOG=debug cargo test -- --nocapture

# Specific test
cargo test test_name

# Benchmarks
cargo bench
```

## Memory Profiling

Use the memory profiler example for leak detection:

```bash
# With dhat
cargo run --example memory_profiler --features dhat-heap

# With Valgrind
cargo build --example memory_profiler --profile profiling
valgrind --leak-check=full ./target/profiling/examples/memory_profiler
```


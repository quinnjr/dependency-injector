# dependency-injector

[![Crates.io](https://img.shields.io/crates/v/dependency-injector.svg)](https://crates.io/crates/dependency-injector)
[![Documentation](https://docs.rs/dependency-injector/badge.svg)](https://docs.rs/dependency-injector)
[![CI](https://github.com/quinnjr/dependency-injector/actions/workflows/ci.yml/badge.svg)](https://github.com/quinnjr/dependency-injector/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/dependency-injector.svg)](LICENSE)

A high-performance, lock-free dependency injection container for Rust.

## Features

- 🚀 **Lock-Free Performance** - Built on `DashMap` for concurrent access without mutex contention
- 🔒 **Thread-Safe** - Safe to use across threads with `Arc<Container>`
- 🎯 **Type-Safe** - Compile-time guarantees with Rust's type system
- 📦 **Multiple Lifetimes** - Singleton, transient, and lazy initialization
- 🌳 **Scoped Containers** - Create child containers with inherited and overridden services
- ⚡ **Zero Config** - No macros required, just plain Rust

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
dependency-injector = "2"
```

### Optional Features

```toml
[dependencies]
dependency-injector = { version = "2", features = ["logging-json"] }
```

| Feature | Description |
|---------|-------------|
| `logging` | Basic debug logging with `tracing` crate (enabled by default) |
| `logging-json` | JSON structured logging output (recommended for production) |
| `logging-pretty` | Colorful pretty logging output (recommended for development) |
| `async` | Async support with Tokio |
| `derive` | Compile-time dependency injection with `#[derive(Inject)]` |

## Quick Start

```rust
use dependency_injector::Container;

// Define your services
#[derive(Clone)]
struct Database {
    url: String,
}

#[derive(Clone)]
struct UserService {
    db: Database,
}

fn main() {
    // Create a container
    let container = Container::new();

    // Register a singleton (created immediately, shared everywhere)
    container.singleton(Database {
        url: "postgres://localhost/mydb".into(),
    });

    // Register with lazy initialization (created on first access)
    let c = container.clone();
    container.lazy(move || UserService {
        db: c.get().unwrap(),
    });

    // Resolve services
    let db = container.get::<Database>().unwrap();
    let users = container.get::<UserService>().unwrap();

    println!("Connected to: {}", db.url);
}
```

## Service Lifetimes

### Singleton

Created immediately and shared across all resolutions:

```rust
container.singleton(Config { debug: true });

let config1 = container.get::<Config>().unwrap();
let config2 = container.get::<Config>().unwrap();
// config1 and config2 point to the same instance
```

### Lazy Singleton

Created on first access, then shared:

```rust
container.lazy(|| ExpensiveService::new());

// Service is created here on first call
let svc = container.get::<ExpensiveService>().unwrap();
```

### Transient

New instance created on every resolution:

```rust
container.transient(|| RequestId(generate_id()));

let id1 = container.get::<RequestId>().unwrap();
let id2 = container.get::<RequestId>().unwrap();
// id1 and id2 are different instances
```

## Scoped Containers

Create child containers that inherit from their parent:

```rust
// Root container with shared services
let root = Container::new();
root.singleton(AppConfig { name: "MyApp".into() });

// Per-request scope
let request_scope = root.scope();
request_scope.singleton(RequestContext { id: "req-123".into() });

// Child can access parent services
assert!(request_scope.contains::<AppConfig>());
assert!(request_scope.contains::<RequestContext>());

// Parent cannot access child services
assert!(!root.contains::<RequestContext>());
```

### Service Overrides

Override parent services in child scopes:

```rust
let root = Container::new();
root.singleton(Database { url: "production".into() });

let test_scope = root.scope();
test_scope.singleton(Database { url: "test".into() });

// Root still has production
let root_db = root.get::<Database>().unwrap();
assert_eq!(root_db.url, "production");

// Test scope has override
let test_db = test_scope.get::<Database>().unwrap();
assert_eq!(test_db.url, "test");
```

## Compile-Time Injection (derive feature)

Use the `#[derive(Inject)]` macro for automatic dependency resolution:

```rust
use dependency_injector::{Container, Inject};
use std::sync::Arc;

#[derive(Clone)]
struct Database {
    url: String,
}

#[derive(Clone)]
struct Cache {
    size: usize,
}

// Derive Inject to generate from_container() method
#[derive(Inject)]
struct UserService {
    #[inject]
    db: Arc<Database>,
    #[inject]
    cache: Arc<Cache>,
    #[inject(optional)]
    logger: Option<Arc<Logger>>,  // Optional dependency
    request_count: u64,            // Uses Default::default()
}

fn main() {
    let container = Container::new();
    container.singleton(Database { url: "postgres://localhost".into() });
    container.singleton(Cache { size: 1024 });

    // Automatically resolve all #[inject] fields
    let service = UserService::from_container(&container).unwrap();
}
```

### Inject Attributes

| Attribute | Field Type | Description |
|-----------|------------|-------------|
| `#[inject]` | `Arc<T>` | Required dependency - fails if not registered |
| `#[inject(optional)]` | `Option<Arc<T>>` | Optional dependency - `None` if not registered |
| (none) | Any type with `Default` | Uses `Default::default()` |

See also the [compile-time typed builder guide](https://quinnjr.github.io/dependency-injector/docs/guide) on the docs site for fully compile-time-checked registration with `TypedBuilder`.

## Framework Integration

### With Armature

[Armature](https://github.com/quinnjr/armature) is a Rust HTTP framework with built-in DI support:

```rust
use armature::prelude::*;

#[injectable]
#[derive(Clone)]
struct Database { url: String }

#[controller("/api")]
struct UserController {
    db: Arc<Database>,
}

#[module]
struct AppModule {
    #[controllers]
    controllers: (UserController,),
    #[providers]
    providers: (Database,),
}
```

> **Note**: This snippet is illustrative only. The full, working example lives in — and is
> maintained and tested by — the [Armature repository](https://github.com/quinnjr/armature);
> it is not compiled as part of this crate.

## API Reference

| Method | Description |
|--------|-------------|
| `Container::new()` | Create a new container |
| `singleton(service)` | Register an immediate singleton |
| `lazy(factory)` | Register a lazy-initialized singleton |
| `transient(factory)` | Register a transient service |
| `get::<T>()` | Resolve a service by type |
| `contains::<T>()` | Check if a service is registered |
| `remove::<T>()` | Remove a service registration |
| `scope()` | Create a child container |

## Documentation

- 📚 **[Full Documentation](https://quinnjr.github.io/dependency-injector/)** - Comprehensive guides and API reference
- 📖 **[docs.rs](https://docs.rs/dependency-injector)** - API documentation
- 📊 **[Benchmarks](https://quinnjr.github.io/dependency-injector/benchmarks)** - Performance metrics

## Logging

Enable structured logging to see what the container is doing:

```rust
use dependency_injector::{Container, logging};

fn main() {
    // Initialize logging (JSON by default with logging-json, pretty with logging-pretty)
    logging::init();

    // Or use the builder for more control
    logging::builder()
        .debug()          // Set log level
        .pretty()         // Use pretty output
        .di_only()        // Only show dependency-injector logs
        .init();

    let container = Container::new();
    // Logs: DEBUG dependency_injector: Creating new root DI container

    container.singleton(MyService { name: "test".into() });
    // Logs: DEBUG dependency_injector: Registering singleton service

    let _ = container.get::<MyService>();
    // Logs: TRACE dependency_injector: Resolving service
}
```

### JSON Output (Production)

```bash
cargo run --features logging-json
```

```json
{"timestamp":"2024-01-01T00:00:00.000Z","level":"DEBUG","fields":{"message":"Creating new root DI container","depth":0},"target":"dependency_injector"}
```

### Pretty Output (Development)

```bash
cargo run --features logging-pretty
```

```text
  2024-01-01T00:00:00.000Z DEBUG dependency_injector: Creating new root DI container, depth: 0
```

## Performance

The container is built for high-performance scenarios:

- **Lock-free reads** using `DashMap`
- **Thread-local hot cache** for sub-20ns resolution
- **Minimal allocations** with `Arc` sharing
- **No runtime reflection** - all type resolution at compile time

### Cross-Language Benchmark Comparison

| Language | Best Library | Singleton Resolution | Mixed Workload (100 ops) |
|----------|-------------|---------------------|--------------------------|
| **Rust** | **dependency-injector** | **17-32 ns** | **2.2 µs** |
| Go | sync.Map | 15 ns | 7 µs |
| C# | MS.Extensions.DI | 208 ns | 31 µs |
| Python | dependency-injector | 95 ns | 15.7 µs |
| Node.js | inversify | 1,829 ns | 15 µs |

**Rust is 6-14x faster** than other languages' popular DI libraries for mixed workloads.

*See [BENCHMARK_COMPARISON.md](BENCHMARK_COMPARISON.md) for full cross-language benchmarks*

### Comparison with Other Rust DI Libraries

| Library | Singleton Resolution | Mixed Workload |
|---------|---------------------|----------------|
| **dependency-injector** | ~17-27 ns | **2.2 µs** |
| shaku | ~17-21 ns | 2.5-15 µs |
| ferrous-di | ~57-70 ns | 7.6-11 µs |

*See [RUST_DI_COMPARISON.md](RUST_DI_COMPARISON.md) for Rust-specific benchmarks*

Run benchmarks locally:

```bash
# Internal benchmarks
cargo bench --bench container_bench

# Comparison against other Rust DI crates
cargo bench --bench comparison_bench
```

## FFI Bindings (Use from Other Languages)

The library provides C-compatible FFI bindings for use from other languages:

### Supported Languages

| Language | Bindings | Example |
|----------|----------|---------|
| **Go** | Native CGO | `ffi/go/` |
| **Python** | ctypes | `ffi/python/` |
| **Node.js** | koffi | `ffi/nodejs/` |
| **C#** | P/Invoke | `ffi/csharp/` |
| **C/C++** | Header file | `ffi/dependency_injector.h` |

### Building FFI Library

```bash
# Build the shared library (cdylib)
cargo rustc --release --features ffi --crate-type cdylib

# Output locations:
# Linux:   target/release/libdependency_injector.so
# macOS:   target/release/libdependency_injector.dylib
# Windows: target/release/dependency_injector.dll
```

### Quick Example (Python)

```python
from dependency_injector import Container

container = Container()
container.register("Config", {"database": "postgres://localhost"})

config = container.resolve("Config")
print(config)  # {'database': 'postgres://localhost'}

container.free()
```

### Quick Example (Go)

```go
package main

import (
    "fmt"

    "github.com/quinnjr/dependency-injector/ffi/go/di"
)

func main() {
    container := di.NewContainer()
    defer container.Free()

    container.RegisterJSON("Config", `{"database": "postgres://localhost"}`)

    config, _ := container.Resolve("Config")
    fmt.Println(string(config))
}
```

*See [ffi/README.md](ffi/README.md) for complete FFI documentation*

## Fuzzing

The project includes comprehensive fuzz testing using `cargo-fuzz`:

```bash
# Install cargo-fuzz (requires nightly)
cargo install cargo-fuzz

# List available fuzz targets
cargo +nightly fuzz list

# Run a specific fuzz target
cargo +nightly fuzz run fuzz_container

# Run for a specific duration
cargo +nightly fuzz run fuzz_container -- -max_total_time=60
```

### Fuzz Targets

| Target | Description |
|--------|-------------|
| `fuzz_container` | Basic registration and resolution operations |
| `fuzz_scoped` | Hierarchical scopes and parent chain resolution |
| `fuzz_concurrent` | Multi-threaded concurrent access patterns |
| `fuzz_lifecycle` | Lazy initialization, transients, and locking |

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) before submitting a PR.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Acknowledgments

Inspired by dependency injection patterns from:
- [Angular](https://angular.io/) - Hierarchical injectors
- [NestJS](https://nestjs.com/) - Module-based DI
- [Microsoft.Extensions.DependencyInjection](https://docs.microsoft.com/en-us/dotnet/core/extensions/dependency-injection) - Service lifetimes


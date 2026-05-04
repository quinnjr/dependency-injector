//! Memory Leak Profiler Example
//!
//! This example exercises the dependency injection container to detect memory leaks.
//! It uses the `dhat` heap profiler to track allocations and identify leaks.
//!
//! # Running with dhat (built-in)
//!
//! ```bash
//! cargo run --example memory_profiler --features dhat-heap
//! # Outputs: dhat-heap.json (view at https://nnethercote.github.io/dh_view/dh_view.html)
//! ```
//!
//! # Running with Valgrind (external)
//!
//! ```bash
//! cargo build --example memory_profiler --profile profiling
//! valgrind --leak-check=full --show-leak-kinds=all \
//!     ./target/profiling/examples/memory_profiler
//! ```
//!
//! # Running with AddressSanitizer (requires nightly)
//!
//! ```bash
//! RUSTFLAGS="-Z sanitizer=address" cargo +nightly run --example memory_profiler --target x86_64-unknown-linux-gnu
//! ```
//!
//! # Running with LeakSanitizer (requires nightly)
//!
//! ```bash
//! RUSTFLAGS="-Z sanitizer=leak" cargo +nightly run --example memory_profiler --target x86_64-unknown-linux-gnu
//! ```

use dependency_injector::Container;
use std::sync::Arc;

// Enable dhat heap profiling when the feature is enabled
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

// === Test Services (concrete types) ===

#[derive(Debug, Clone)]
struct ConsoleLogger {
    prefix: String,
}

impl ConsoleLogger {
    fn log(&self, message: &str) {
        // Suppress actual output during profiling
        let _ = (self.prefix.as_str(), message);
    }
}

#[derive(Debug, Clone)]
struct PostgresDb {
    connection_string: String,
    logger: Arc<ConsoleLogger>,
}

impl PostgresDb {
    fn query(&self, sql: &str) -> Vec<String> {
        self.logger.log(&format!("Executing: {}", sql));
        vec![format!("Result from {}", self.connection_string)]
    }
}

#[derive(Debug, Clone)]
struct DbUserRepository {
    db: Arc<PostgresDb>,
}

impl DbUserRepository {
    fn find_user(&self, id: u64) -> Option<String> {
        let results = self
            .db
            .query(&format!("SELECT * FROM users WHERE id = {}", id));
        results.into_iter().next()
    }
}

// The warning related to unused fields: name, data
// Since these are kept for demonstration purposes or potential future use,
// we can suppress the warning here.
#[allow(dead_code)]
struct CacheService {
    name: String,
    data: Vec<u8>,
}

// The warning related to unused fields: session_id
// Since this is a mock session manager, we ignore the unused warning.
#[allow(dead_code)]
struct SessionManager {
    session_id: String,
}

// The warning related to unused fields: logger
// Since this is a mock service, we ignore the unused warning.
#[allow(dead_code)]
struct AuthService {
    logger: Arc<ConsoleLogger>,
}

// === Profiling Scenarios ===

/// Test singleton registration and resolution
fn profile_singletons(iterations: usize) {
    println!("\n=== Profiling Singletons ({} iterations) ===", iterations);

    for i in 0..iterations {
        let container = Container::new();

        // Register singleton services
        container.singleton(ConsoleLogger {
            prefix: "APP".to_string(),
        });

        // Resolve multiple times (should return same Arc)
        for _ in 0..10 {
            let _logger: Arc<ConsoleLogger> = container.get().unwrap();
        }

        if i % 100 == 0 {
            println!("  Iteration {}", i);
        }
    }
}

/// Test lazy singleton creation
fn profile_lazy_singletons(iterations: usize) {
    println!(
        "\n=== Profiling Lazy Singletons ({} iterations) ===",
        iterations
    );

    for i in 0..iterations {
        let container = Container::new();

        // Register lazy singleton
        container.lazy(|| ConsoleLogger {
            prefix: "LAZY".to_string(),
        });

        // First resolution triggers creation
        let _logger: Arc<ConsoleLogger> = container.get().unwrap();

        // Subsequent resolutions return cached instance
        for _ in 0..10 {
            let _logger: Arc<ConsoleLogger> = container.get().unwrap();
        }

        if i % 100 == 0 {
            println!("  Iteration {}", i);
        }
    }
}

/// Test transient service creation
fn profile_transients(iterations: usize) {
    println!("\n=== Profiling Transients ({} iterations) ===", iterations);

    let container = Container::new();

    // Register transient service
    container.transient(|| ConsoleLogger {
        prefix: format!("TRANSIENT-{}", rand_id()),
    });

    for i in 0..iterations {
        // Each resolution creates a new instance
        let _logger: Arc<ConsoleLogger> = container.get().unwrap();

        if i % 1000 == 0 {
            println!("  Iteration {}", i);
        }
    }
}

/// Test scope creation and destruction
fn profile_scopes(iterations: usize) {
    println!("\n=== Profiling Scopes ({} iterations) ===", iterations);

    let container = Container::new();

    // Register singleton in root
    container.singleton(ConsoleLogger {
        prefix: "ROOT".to_string(),
    });

    for i in 0..iterations {
        // Create child scope
        let scope = container.scope();

        // Register scoped service
        scope.singleton(PostgresDb {
            connection_string: format!("postgres://scope-{}", i),
            logger: container.get().unwrap(),
        });

        // Resolve services within scope
        let _db: Arc<PostgresDb> = scope.get().unwrap();
        let _logger: Arc<ConsoleLogger> = scope.get().unwrap();

        // Scope is dropped here - should clean up properly
        drop(scope);

        if i % 100 == 0 {
            println!("  Iteration {}", i);
        }
    }
}

/// Test nested scopes
fn profile_nested_scopes(iterations: usize) {
    println!(
        "\n=== Profiling Nested Scopes ({} iterations) ===",
        iterations
    );

    let container = Container::new();

    container.singleton(ConsoleLogger {
        prefix: "ROOT".to_string(),
    });

    for i in 0..iterations {
        let scope1 = container.scope();

        scope1.singleton(PostgresDb {
            connection_string: "postgres://scope1".to_string(),
            logger: container.get().unwrap(),
        });

        // Create nested scope from scope1
        let scope2 = scope1.scope();

        scope2.singleton(DbUserRepository {
            db: scope1.get().unwrap(),
        });

        // Resolve in child scope
        let _repo: Arc<DbUserRepository> = scope2.get().unwrap();

        // Create another nested scope
        let scope3 = scope2.scope();
        let _repo2: Arc<DbUserRepository> = scope3.get().unwrap();

        // All scopes dropped here
        drop(scope3);
        drop(scope2);
        drop(scope1);

        if i % 100 == 0 {
            println!("  Iteration {}", i);
        }
    }
}

/// Test complex dependency graph
fn profile_complex_dependencies(iterations: usize) {
    println!(
        "\n=== Profiling Complex Dependencies ({} iterations) ===",
        iterations
    );

    for i in 0..iterations {
        let container = Container::new();

        // Build a dependency chain: DbUserRepository -> PostgresDb -> ConsoleLogger
        container.singleton(ConsoleLogger {
            prefix: "COMPLEX".to_string(),
        });

        let logger = container.get::<ConsoleLogger>().unwrap();

        container.singleton(PostgresDb {
            connection_string: "postgres://complex".to_string(),
            logger,
        });

        let db = container.get::<PostgresDb>().unwrap();

        container.singleton(DbUserRepository { db });

        // Resolve the full dependency chain
        let repo: Arc<DbUserRepository> = container.get().unwrap();
        let _ = repo.find_user(i as u64);

        // Resolve same services again (should use cached instances)
        let _db: Arc<PostgresDb> = container.get().unwrap();
        let _logger: Arc<ConsoleLogger> = container.get().unwrap();

        if i % 100 == 0 {
            println!("  Iteration {}", i);
        }
    }
}

/// Test rapid container creation/destruction
fn profile_container_lifecycle(iterations: usize) {
    println!(
        "\n=== Profiling Container Lifecycle ({} iterations) ===",
        iterations
    );

    for i in 0..iterations {
        let container = Container::new();

        container.singleton(ConsoleLogger {
            prefix: "LIFECYCLE".to_string(),
        });

        container.transient(|| PostgresDb {
            connection_string: "postgres://lifecycle".to_string(),
            logger: Arc::new(ConsoleLogger {
                prefix: "INLINE".to_string(),
            }),
        });

        // Use the container
        let _logger: Arc<ConsoleLogger> = container.get().unwrap();
        let _db: Arc<PostgresDb> = container.get().unwrap();

        // Container dropped here
        drop(container);

        if i % 100 == 0 {
            println!("  Iteration {}", i);
        }
    }
}

/// Test large allocations with cache-like service
fn profile_large_allocations(iterations: usize) {
    println!(
        "\n=== Profiling Large Allocations ({} iterations) ===",
        iterations
    );

    for i in 0..iterations {
        let container = Container::new();

        // Create a service with significant memory allocation
        container.singleton(CacheService {
            name: format!("cache-{}", i),
            data: vec![0u8; 1024], // 1KB per cache
        });

        let _cache: Arc<CacheService> = container.get().unwrap();

        // Create scopes with their own caches
        let scope = container.scope();
        scope.singleton(CacheService {
            name: format!("scope-cache-{}", i),
            data: vec![0u8; 512], // 512 bytes per scope cache
        });

        let _scope_cache: Arc<CacheService> = scope.get().unwrap();

        drop(scope);
        drop(container);

        if i % 100 == 0 {
            println!("  Iteration {}", i);
        }
    }
}

/// Test concurrent-like access patterns (single-threaded simulation)
fn profile_access_patterns(iterations: usize) {
    println!(
        "\n=== Profiling Access Patterns ({} iterations) ===",
        iterations
    );

    let container = Container::new();

    container.singleton(ConsoleLogger {
        prefix: "ACCESS".to_string(),
    });

    container.lazy(|| SessionManager {
        session_id: format!("session-{}", rand_id()),
    });

    container.singleton(AuthService {
        logger: Arc::new(ConsoleLogger {
            prefix: "AUTH".to_string(),
        }),
    });

    for i in 0..iterations {
        // Simulate request handling pattern
        let scope = container.scope();

        // Access various services in different orders
        let _logger: Arc<ConsoleLogger> = scope.get().unwrap();
        let _session: Arc<SessionManager> = scope.get().unwrap();
        let _auth: Arc<AuthService> = scope.get().unwrap();

        // Re-access in different order
        let _auth2: Arc<AuthService> = scope.get().unwrap();
        let _logger2: Arc<ConsoleLogger> = scope.get().unwrap();

        if i % 500 == 0 {
            println!("  Iteration {}", i);
        }
    }
}

/// Generate a pseudo-random ID for testing
fn rand_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn main() {
    // Initialize dhat profiler if enabled
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║           Memory Leak Profiler for dependency-injector     ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║ This runs various scenarios to detect memory leaks.        ║");
    println!("║ Use with dhat, Valgrind, or sanitizers for full analysis.  ║");
    println!("╚════════════════════════════════════════════════════════════╝");

    // Run profiling scenarios
    profile_singletons(500);
    profile_lazy_singletons(500);
    profile_transients(5000);
    profile_scopes(500);
    profile_nested_scopes(200);
    profile_complex_dependencies(500);
    profile_container_lifecycle(500);
    profile_large_allocations(300);
    profile_access_patterns(2000);

    println!("\n✅ All profiling scenarios completed!");

    #[cfg(feature = "dhat-heap")]
    {
        println!("\n📊 dhat heap profile written to: dhat-heap.json");
        println!("   View at: https://nnethercote.github.io/dh_view/dh_view.html");
    }

    #[cfg(not(feature = "dhat-heap"))]
    {
        println!("\n💡 Tip: Run with --features dhat-heap for detailed heap profiling");
        println!("   cargo run --example memory_profiler --features dhat-heap");
    }
}

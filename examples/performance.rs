//! Example demonstrating the performance-oriented features:
//! scope pooling, cache warming, container locking, and frozen storage.
//!
//! Run with:
//!   cargo run --example performance
//!
//! The FrozenStorage section requires the `perfect-hash` feature:
//!   cargo run --example performance --features perfect-hash

use dependency_injector::{Container, ScopePool};

#[derive(Clone)]
struct AppConfig {
    name: String,
}

#[derive(Clone)]
struct Database {
    url: String,
}

#[derive(Clone)]
struct RequestId(u64);

fn main() {
    println!("=== Dependency Injector Performance Demo ===\n");

    let root = Container::new();
    root.singleton(AppConfig {
        name: "MyApp".into(),
    });
    root.singleton(Database {
        url: "postgres://localhost:5432/app".into(),
    });

    // --- ScopePool: reusable request scopes ---
    // Creating a scope allocates; a pool recycles those allocations so a
    // request handler pays ~20ns per acquire instead of ~134ns per fresh scope.
    println!("ScopePool with 4 pre-allocated scopes...");
    let pool = ScopePool::new(&root, 4);
    println!("  available scopes: {}", pool.available_count());

    {
        let scope = pool.acquire();
        println!("  after acquire: {} available", pool.available_count());

        // A pooled scope is a normal child scope of the pool's parent
        scope.singleton(RequestId(1));
        let id = scope.get::<RequestId>().expect("registered in this scope");
        assert!(scope.contains::<AppConfig>());
        println!("  request {} sees parent AppConfig: true", id.0);

        // Dropping the PooledScope clears it and returns it to the pool
    }
    println!("  after drop: {} available", pool.available_count());

    // Reused scopes always start clean - values never leak across acquisitions
    {
        let scope = pool.acquire();
        assert!(!scope.contains::<RequestId>());
        println!(
            "  reused scope still holds RequestId: {}",
            scope.contains::<RequestId>()
        );
    }
    println!();

    // --- Thread-local hot cache ---
    println!("Warming the thread-local hot cache...");
    // warm_cache resolves once so later get() calls on this thread hit the
    // per-thread cache instead of the concurrent map
    root.warm_cache::<Database>();
    root.warm_cache::<AppConfig>();
    let db = root.get::<Database>().expect("served from the hot cache");
    println!("  resolved {} via warmed cache", db.url);

    // clear_cache drops this thread's cached Arcs. Registrations, removals,
    // and clears invalidate the cache automatically, so this is only needed
    // for explicit control (e.g. releasing cached Arcs early).
    root.clear_cache();
    println!("  hot cache cleared\n");

    // --- Locking ---
    println!("Locking the container after startup...");
    root.lock();
    println!("  root locked: {}", root.is_locked());
    // Registering on a locked container panics; resolution is unaffected:
    let config = root.get::<AppConfig>().expect("resolution still works");
    println!("  {} still resolvable after lock\n", config.name);

    // --- FrozenStorage: perfect-hash lookups ---
    #[cfg(feature = "perfect-hash")]
    {
        use std::any::TypeId;
        use std::sync::Arc;

        println!("Freezing the container (perfect-hash)...");
        // freeze() locks the container and snapshots it into a FrozenStorage
        // backed by a minimal perfect hash function: collision-free O(1)
        // lookups, ~5ns faster than the dynamic path.
        let frozen = root.freeze();
        println!("  frozen {} services", frozen.len());

        let entry = frozen
            .resolve(&TypeId::of::<Database>())
            .expect("Database is in the frozen storage");
        let frozen_db: Arc<Database> = entry.downcast().expect("stored as Database");
        println!("  frozen lookup -> {}", frozen_db.url);

        assert!(frozen.contains(&TypeId::of::<AppConfig>()));
        assert!(!frozen.contains(&TypeId::of::<RequestId>()));
        println!("  contains(AppConfig) = true, contains(RequestId) = false\n");
    }

    #[cfg(not(feature = "perfect-hash"))]
    println!(
        "FrozenStorage section skipped - run with:\n  \
         cargo run --example performance --features perfect-hash\n"
    );

    println!("=== Demo Complete ===");
    println!("\nPerformance toolbox:");
    println!("  - ScopePool: recycle request scopes (~20ns acquire vs ~134ns fresh)");
    println!("  - warm_cache/clear_cache: control the thread-local hot cache");
    println!("  - lock(): forbid registrations after startup");
    println!("  - freeze(): perfect-hash storage for collision-free O(1) lookups");
}

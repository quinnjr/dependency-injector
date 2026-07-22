//! Example demonstrating the compile-time safety surface:
//! `TypedBuilder`/`TypedContainer`, type-level `Require` lists, and the
//! verified `Service`/`ServiceProvider`/`ServiceModule` traits.
//!
//! Run with:
//!   cargo run --example typed_builder
//!
//! Everything here is hand-written, so no extra features are needed. With the
//! `derive` feature enabled, the derive macros generate the same impls:
//!   - `#[derive(Service)]`      => the `Service` impl written below
//!   - `#[derive(TypedRequire)]` => the `Require` impl written below
//!     (with `#[dep]` / `#[dep(optional)]` field attributes)

use dependency_injector::Container;
use dependency_injector::typed::{Reg, Require, TypedBuilder, TypedContainer};
use dependency_injector::verified::{Service, ServiceModule, ServiceProvider};
use std::sync::Arc;

#[derive(Clone)]
struct Config {
    debug: bool,
}

#[derive(Clone)]
struct Database {
    url: String,
}

#[derive(Clone)]
struct Cache {
    size: usize,
}

// A service that declares its dependencies as a type-level list. In a real
// application it would hold `Arc<Database>` / `Arc<Cache>` fields.
#[derive(Clone)]
struct UserService;

impl Require for UserService {
    // Reg is a cons-list: Reg<Head, Tail> terminated by ().
    type Dependencies = Reg<Database, Reg<Cache, ()>>;
}

// --- Verified services: dependencies declared through the Service trait ---

impl Service for Config {
    type Dependencies = ();

    fn create(_: ()) -> Self {
        Config { debug: true }
    }
}

impl Service for Database {
    type Dependencies = Arc<Config>;

    fn create(config: Arc<Config>) -> Self {
        Database {
            url: if config.debug {
                "debug://localhost".into()
            } else {
                "prod://server".into()
            },
        }
    }
}

// Dependency tuples (up to 12 elements) may mix required and optional entries.
#[derive(Clone)]
struct UserRepository {
    db: Arc<Database>,
    cache: Option<Arc<Cache>>,
}

impl Service for UserRepository {
    type Dependencies = (Arc<Database>, Option<Arc<Cache>>);

    fn create((db, cache): Self::Dependencies) -> Self {
        UserRepository { db, cache }
    }
}

// --- ServiceModule: group related registrations behind one call ---

struct DataModule;

impl ServiceModule for DataModule {
    fn register(container: &Container) {
        container.provide::<Config>();
        container.provide::<Database>();
    }
}

/// Compile-time check that `S::Dependencies` is exactly `D`.
fn assert_dependencies<S: Require<Dependencies = D>, D>() {}

/// Compile-time check that `S::Dependencies` matches a built registry type.
fn assert_matches_registry<S: Require<Dependencies = R>, R>(_: &TypedContainer<R>) {}

fn main() {
    println!("=== Dependency Injector Compile-Time Safety Demo ===\n");

    // --- TypedBuilder / TypedContainer ---
    println!("Building a TypedContainer...");
    let typed = TypedBuilder::new()
        .singleton(Config { debug: true })
        .singleton(Database {
            url: "postgres://localhost:5432/app".into(),
        })
        .lazy(|| Cache { size: 1024 })
        .build(); // build() locks the container - no further registrations

    // get() returns Arc<T> directly - no Result to unwrap
    let db = typed.get::<Database>();
    let cache = typed.get::<Cache>();
    println!("  db = {}, cache size = {}", db.url, cache.size);
    assert!(typed.contains::<Config>());
    assert!(typed.try_get::<UserRepository>().is_none());

    // A typed container still hands out dynamic child scopes
    let child = typed.scope();
    child.singleton(Cache { size: 64 });
    let small = child.get::<Cache>().expect("override in child scope");
    println!("  child scope overrides cache size: {}\n", small.size);

    // --- Require: type-level dependency lists ---
    println!("Verifying UserService dependencies at compile time...");
    // Registration order builds the registry type head-first:
    // singleton(Cache) then singleton(Database) => Reg<Database, Reg<Cache, ()>>
    let verified = TypedBuilder::new()
        .singleton(Cache { size: 256 })
        .singleton(Database {
            url: "postgres://replica".into(),
        })
        .build();

    // These are compile-time proofs: drop a registration (or reorder) and
    // this example stops compiling.
    assert_dependencies::<UserService, Reg<Database, Reg<Cache, ()>>>();
    assert_matches_registry::<UserService, _>(&verified);
    println!("  registry matches UserService::Dependencies\n");

    // --- Service + ServiceProvider: auto-wired registration ---
    println!("Providing verified services...");
    let container = Container::new();
    container.provide::<Config>(); // lazy: created on first resolve
    container.provide::<Database>(); // deps resolved from the container

    // provide_singleton is eager: dependencies are resolved right now and
    // it returns false (instead of panicking later) if a required one is missing
    let registered = container.provide_singleton::<UserRepository>();
    println!("  UserRepository registered: {registered}");

    let repo = container.get::<UserRepository>().expect("provided above");
    println!(
        "  repo uses {} (optional cache attached: {})",
        repo.db.url,
        repo.cache.is_some()
    );

    // Missing *required* dependency -> provide_singleton fails fast
    let empty = Container::new();
    assert!(!empty.provide_singleton::<UserRepository>());
    println!("  empty container rejects UserRepository: missing Database\n");

    // --- ServiceModule: install a whole subsystem at once ---
    println!("Registering DataModule...");
    let modular = Container::new();
    DataModule::register(&modular);
    assert!(modular.contains::<Config>());
    assert!(modular.contains::<Database>());
    let db = modular.get::<Database>().expect("registered by the module");
    println!("  module provided database: {}\n", db.url);

    println!("=== Demo Complete ===");
    println!("\nCompile-time safety layers:");
    println!("  - TypedBuilder tracks registrations in its type parameter");
    println!("  - Require declares deps as a type-level Reg list");
    println!("  - Service/ServiceProvider auto-resolve declared dependencies");
    println!("  - ServiceModule groups registrations behind one call");
}

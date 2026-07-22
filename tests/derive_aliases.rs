//! Tests that `#[inject]` and `#[dep]` are exact aliases across all three
//! derive macros (`Inject`, `Service`, and `TypedRequire`).
//!
//! Run with:
//!   cargo test --test derive_aliases --features derive
#![cfg(feature = "derive")]

use dependency_injector::typed::{Reg, Require};
use dependency_injector::verified::ServiceProvider;
use dependency_injector::{Container, Inject, Service, TypedRequire};
use std::sync::Arc;

#[derive(Clone)]
struct Database {
    url: &'static str,
}

#[derive(Clone)]
struct Cache;

// -----------------------------------------------------------------------------
// Inject: `#[dep]` spelling
// -----------------------------------------------------------------------------

#[derive(Inject)]
struct DepSpelledInject {
    #[dep]
    db: Arc<Database>,
    #[dep(optional)]
    cache: Option<Arc<Cache>>,
    // Fields without a dependency marker fall back to Default.
    counter: u64,
}

#[test]
fn inject_accepts_dep_spelling() {
    let container = Container::new();
    container.singleton(Database {
        url: "postgres://localhost",
    });

    let service = DepSpelledInject::from_container(&container).unwrap();
    assert_eq!(service.db.url, "postgres://localhost");
    assert!(service.cache.is_none());
    assert_eq!(service.counter, 0);
}

#[test]
fn inject_accepts_dep_optional_spelling() {
    let container = Container::new();
    container.singleton(Database {
        url: "postgres://localhost",
    });
    container.singleton(Cache);

    let service = DepSpelledInject::from_container(&container).unwrap();
    assert!(service.cache.is_some());
}

// -----------------------------------------------------------------------------
// Service: `#[inject]` spelling
// -----------------------------------------------------------------------------

#[derive(Clone, Service)]
struct InjectSpelledService {
    #[inject]
    db: Arc<Database>,
    #[inject(optional)]
    cache: Option<Arc<Cache>>,
    // Fields without a dependency marker fall back to Default.
    request_count: u64,
}

#[test]
fn service_accepts_inject_spelling() {
    let container = Container::new();
    container.singleton(Database {
        url: "postgres://localhost",
    });
    container.provide::<InjectSpelledService>();

    let service = container.get::<InjectSpelledService>().unwrap();
    assert_eq!(service.db.url, "postgres://localhost");
    assert!(service.cache.is_none());
    assert_eq!(service.request_count, 0);
}

#[test]
fn service_accepts_inject_optional_spelling() {
    let container = Container::new();
    container.singleton(Database {
        url: "postgres://localhost",
    });
    container.singleton(Cache);
    container.provide::<InjectSpelledService>();

    let service = container.get::<InjectSpelledService>().unwrap();
    assert!(service.cache.is_some());
}

// -----------------------------------------------------------------------------
// TypedRequire: `#[inject]` spelling
// -----------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, TypedRequire)]
struct InjectSpelledRequire {
    #[inject]
    db: Arc<Database>,
    #[inject]
    cache: Arc<Cache>,
}

#[allow(dead_code)]
#[derive(Clone, TypedRequire)]
struct InjectSpelledWithOptional {
    #[inject]
    db: Arc<Database>,
    // Optional deps are excluded from the required-dependency list,
    // regardless of spelling.
    #[inject(optional)]
    cache: Option<Arc<Cache>>,
}

#[allow(dead_code)]
#[derive(Clone, TypedRequire)]
struct MixedSpelling {
    #[inject]
    db: Arc<Database>,
    #[dep]
    cache: Arc<Cache>,
}

/// Compile-time check that `S::Dependencies` is exactly `D`.
fn assert_dependencies<S: Require<Dependencies = D>, D>() {}

#[test]
fn typed_require_accepts_inject_spelling() {
    assert_dependencies::<InjectSpelledRequire, Reg<Database, Reg<Cache, ()>>>();
}

#[test]
fn typed_require_excludes_optional_inject_deps() {
    assert_dependencies::<InjectSpelledWithOptional, Reg<Database, ()>>();
}

#[test]
fn typed_require_allows_mixed_spellings() {
    assert_dependencies::<MixedSpelling, Reg<Database, Reg<Cache, ()>>>();
}

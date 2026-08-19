//! # Armature DI - High-Performance Dependency Injection for Rust
//!
//! A lightning-fast, type-safe dependency injection container optimized for
//! real-world web framework usage.
//!
//! ## Features
//!
//! - ⚡ **Lock-free** - Uses `DashMap` for concurrent access without blocking
//! - 🔒 **Type-safe** - Compile-time type checking with zero runtime overhead
//! - 🚀 **Zero-config** - Any `Send + Sync + 'static` type is automatically injectable
//! - 🔄 **Scoped containers** - Hierarchical scopes with full parent chain resolution
//! - 🏭 **Lazy singletons** - Services created on first access
//! - ♻️ **Transient services** - Fresh instance on every resolve
//! - 🧵 **Thread-local cache** - Hot path optimization for frequently accessed services
//! - 📊 **Observable** - Optional tracing integration with JSON or pretty output
//! - ⏳ **Async singletons** - Optional `async` feature adds `lazy_async`/`get_async` (tokio)
//!
//! ## Quick Start
//!
//! ```rust
//! use dependency_injector::Container;
//!
//! // Any Send + Sync + 'static type works - no boilerplate!
//! #[derive(Clone)]
//! struct Database {
//!     url: String,
//! }
//!
//! #[derive(Clone)]
//! struct UserService {
//!     db: Database,
//! }
//!
//! let container = Container::new();
//!
//! // Register services
//! container.singleton(Database { url: "postgres://localhost".into() });
//! container.singleton(UserService {
//!     db: Database { url: "postgres://localhost".into() }
//! });
//!
//! // Resolve - returns Arc<T> for zero-copy sharing
//! let db = container.get::<Database>().unwrap();
//! let users = container.get::<UserService>().unwrap();
//! ```
//!
//! ## Service Lifetimes
//!
//! ```rust
//! use dependency_injector::Container;
//! use std::sync::atomic::{AtomicU64, Ordering};
//!
//! static COUNTER: AtomicU64 = AtomicU64::new(0);
//!
//! #[derive(Clone, Default)]
//! struct Config { debug: bool }
//!
//! #[derive(Clone)]
//! struct RequestId(u64);
//!
//! let container = Container::new();
//!
//! // Singleton - one instance, shared everywhere
//! container.singleton(Config { debug: true });
//!
//! // Lazy singleton - created on first access
//! container.lazy(|| Config { debug: false });
//!
//! // Transient - new instance every time
//! container.transient(|| RequestId(COUNTER.fetch_add(1, Ordering::SeqCst)));
//! ```
//!
//! ## Scoped Containers
//!
//! ```rust
//! use dependency_injector::Container;
//!
//! #[derive(Clone)]
//! struct AppConfig { name: String }
//!
//! #[derive(Clone)]
//! struct RequestContext { id: String }
//!
//! // Root container with app-wide services
//! let root = Container::new();
//! root.singleton(AppConfig { name: "MyApp".into() });
//!
//! // Per-request scope - inherits from root
//! let request_scope = root.scope();
//! request_scope.singleton(RequestContext { id: "req-123".into() });
//!
//! // Request scope can access root services
//! assert!(request_scope.contains::<AppConfig>());
//! assert!(request_scope.contains::<RequestContext>());
//!
//! // Root cannot access request-scoped services
//! assert!(!root.contains::<RequestContext>());
//! ```
//!
//! ## Performance
//!
//! - **Lock-free reads**: Using `DashMap` for ~10x faster concurrent access vs `RwLock`
//! - **AHash**: Faster hashing for `TypeId` keys
//! - **Thread-local cache**: Avoid map lookups for hot services
//! - **Zero allocation resolve**: Returns `Arc<T>` directly, no cloning

#[cfg(feature = "async")]
pub mod async_support;
mod container;
mod error;
mod factory;
#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(feature = "logging")]
pub mod logging;
mod provider;
mod scope;
mod storage;
pub mod typed;
pub mod verified;

// Re-export FrozenStorage when perfect-hash feature is enabled
#[cfg(feature = "perfect-hash")]
pub use storage::FrozenStorage;

pub use container::*;
pub use error::*;
pub use factory::*;
pub use provider::*;
pub use scope::*;

// Re-export tracing macros for convenience when logging feature is enabled
#[cfg(feature = "logging")]
pub use tracing::{debug, error, info, trace, warn};

// Re-export derive macros when feature is enabled
#[cfg(feature = "derive")]
pub use dependency_injector_derive::{Inject, Service, TypedRequire};

// Re-export for convenience
pub use std::sync::Arc;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::{
        BatchBuilder, BatchRegistrar, Container, DiError, Factory, Injectable, Lifetime,
        PooledScope, Provider, Result, Scope, ScopePool, ScopedContainer,
    };
    pub use std::sync::Arc;

    // Compile-time safety types
    pub use crate::typed::{
        DeclaresDeps, DepsPresent, Has, HasService, HasType, Reg, TypedBuilder, TypedContainer,
    };
    pub use crate::verified::{Resolvable, Service, ServiceModule, ServiceProvider};

    #[cfg(feature = "async")]
    pub use crate::async_support::AsyncLazy;

    #[cfg(feature = "derive")]
    pub use crate::{Inject, Service as ServiceDerive, TypedRequire};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Clone)]
    struct Database {
        url: String,
    }

    #[allow(dead_code)]
    #[derive(Clone)]
    struct UserService {
        name: String,
    }

    #[test]
    fn test_singleton_registration() {
        let container = Container::new();
        container.singleton(Database { url: "test".into() });

        let db = container.get::<Database>().unwrap();
        assert_eq!(db.url, "test");
    }

    #[test]
    fn test_multiple_resolve_same_instance() {
        let container = Container::new();
        container.singleton(Database { url: "test".into() });

        let db1 = container.get::<Database>().unwrap();
        let db2 = container.get::<Database>().unwrap();

        // Same Arc instance
        assert!(Arc::ptr_eq(&db1, &db2));
    }

    #[test]
    fn test_transient_creates_new_instance() {
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        #[derive(Clone)]
        struct Counter(u32);

        let container = Container::new();
        container.transient(|| Counter(COUNTER.fetch_add(1, Ordering::SeqCst)));

        let c1 = container.get::<Counter>().unwrap();
        let c2 = container.get::<Counter>().unwrap();

        assert_ne!(c1.0, c2.0);
    }

    #[test]
    fn test_lazy_singleton() {
        static CREATED: AtomicU32 = AtomicU32::new(0);

        #[derive(Clone)]
        struct LazyService;

        let container = Container::new();
        container.lazy(|| {
            CREATED.fetch_add(1, Ordering::SeqCst);
            LazyService
        });

        assert_eq!(CREATED.load(Ordering::SeqCst), 0);

        let _ = container.get::<LazyService>().unwrap();
        assert_eq!(CREATED.load(Ordering::SeqCst), 1);

        // Second resolve doesn't create new instance
        let _ = container.get::<LazyService>().unwrap();
        assert_eq!(CREATED.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_scoped_container() {
        let root = Container::new();
        root.singleton(Database { url: "root".into() });

        let child = root.scope();
        child.singleton(UserService {
            name: "child".into(),
        });

        // Child can access root services
        assert!(child.contains::<Database>());
        assert!(child.contains::<UserService>());

        // Root cannot access child services
        assert!(root.contains::<Database>());
        assert!(!root.contains::<UserService>());
    }

    #[test]
    fn test_not_found_error() {
        let container = Container::new();
        let result = container.get::<Database>();
        assert!(result.is_err());
    }

    #[test]
    fn test_override_in_scope() {
        let root = Container::new();
        root.singleton(Database {
            url: "production".into(),
        });

        let test_scope = root.scope();
        test_scope.singleton(Database { url: "test".into() });

        // Root has production
        let root_db = root.get::<Database>().unwrap();
        assert_eq!(root_db.url, "production");

        // Child has test override
        let child_db = test_scope.get::<Database>().unwrap();
        assert_eq!(child_db.url, "test");
    }
}

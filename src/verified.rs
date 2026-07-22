//! Verified Service Providers
//!
//! This module provides traits for services that declare their dependencies
//! at compile time, enabling static verification of dependency graphs.
//!
//! # Features
//!
//! - **`Service` trait**: Declare dependencies and creation logic
//! - **`ServiceProvider` trait**: Auto-register services with their factories
//! - **Compile-time cycle detection**: The type system prevents circular deps
//!
//! # Example
//!
//! ```rust
//! use dependency_injector::verified::{Service, ServiceProvider};
//! use dependency_injector::Container;
//! use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct Database {
//!     url: String,
//! }
//!
//! impl Service for Database {
//!     type Dependencies = ();
//!
//!     fn create(_deps: Self::Dependencies) -> Self {
//!         Database { url: "postgres://localhost".into() }
//!     }
//! }
//!
//! #[derive(Clone)]
//! struct UserRepository {
//!     db: Arc<Database>,
//! }
//!
//! impl Service for UserRepository {
//!     type Dependencies = Arc<Database>;
//!
//!     fn create(db: Self::Dependencies) -> Self {
//!         UserRepository { db }
//!     }
//! }
//!
//! // Auto-register with dependencies resolved
//! let container = Container::new();
//! container.provide::<Database>();
//! container.provide::<UserRepository>();
//!
//! let repo = container.get::<UserRepository>().unwrap();
//! ```

use crate::{Container, Injectable};
use std::sync::Arc;

// =============================================================================
// Service Trait
// =============================================================================

/// A service that declares its dependencies at compile time.
///
/// The `Dependencies` associated type specifies what the service needs,
/// and `create` defines how to construct the service given those dependencies.
///
/// # Supported Dependency Types
///
/// - `()` - No dependencies
/// - `Arc<T>` - Single required dependency
/// - `(Arc<A>, Arc<B>)` - Multiple dependencies (tuples up to 12)
/// - `Option<Arc<T>>` - Optional dependency
/// - `(Arc<A>, Option<Arc<B>>)` - Tuples may mix required and optional
///
/// # Example
///
/// ```rust
/// use dependency_injector::verified::Service;
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct Config {
///     debug: bool,
/// }
///
/// impl Service for Config {
///     type Dependencies = ();
///
///     fn create(_: ()) -> Self {
///         Config { debug: false }
///     }
/// }
///
/// #[derive(Clone)]
/// struct Logger {
///     config: Arc<Config>,
/// }
///
/// impl Service for Logger {
///     type Dependencies = Arc<Config>;
///
///     fn create(config: Arc<Config>) -> Self {
///         Logger { config }
///     }
/// }
/// ```
pub trait Service: Injectable + Sized {
    /// The dependencies required to create this service.
    ///
    /// Use `()` for no dependencies, `Arc<T>` for one, or tuples for multiple.
    type Dependencies: Resolvable;

    /// Create a new instance given the resolved dependencies.
    fn create(deps: Self::Dependencies) -> Self;
}

// =============================================================================
// Resolvable Trait - Dependencies that can be resolved from a container
// =============================================================================

/// Trait for types that can be resolved from a container.
///
/// This is automatically implemented for:
/// - `()` - No dependencies
/// - `Arc<T>` - Single service
/// - `Option<Arc<T>>` - Optional service
/// - Tuples of resolvable elements - Multiple services, mixing required
///   and optional dependencies as needed
pub trait Resolvable: Sized {
    /// Resolve this dependency from the container.
    ///
    /// Returns `None` if any required dependency is missing.
    fn resolve(container: &Container) -> Option<Self>;
}

// No dependencies
impl Resolvable for () {
    #[inline]
    fn resolve(_container: &Container) -> Option<Self> {
        Some(())
    }
}

// Single dependency
impl<T: Injectable> Resolvable for Arc<T> {
    #[inline]
    fn resolve(container: &Container) -> Option<Self> {
        container.try_get::<T>()
    }
}

// Optional dependency
impl<T: Injectable> Resolvable for Option<Arc<T>> {
    #[inline]
    fn resolve(container: &Container) -> Option<Self> {
        Some(container.try_get::<T>())
    }
}

// Tuple implementations (2-12 elements)
//
// Each element only needs to be `Resolvable` itself, so tuples may freely mix
// required (`Arc<T>`) and optional (`Option<Arc<T>>`) dependencies. A missing
// required element fails resolution of the whole tuple; a missing optional
// element resolves to `None`.
macro_rules! impl_resolvable_tuple {
    ($($T:ident),+) => {
        impl<$($T: Resolvable),+> Resolvable for ($($T,)+) {
            #[inline]
            fn resolve(container: &Container) -> Option<Self> {
                Some(($($T::resolve(container)?,)+))
            }
        }
    };
}

impl_resolvable_tuple!(A, B);
impl_resolvable_tuple!(A, B, C);
impl_resolvable_tuple!(A, B, C, D);
impl_resolvable_tuple!(A, B, C, D, E);
impl_resolvable_tuple!(A, B, C, D, E, F);
impl_resolvable_tuple!(A, B, C, D, E, F, G);
impl_resolvable_tuple!(A, B, C, D, E, F, G, H);
impl_resolvable_tuple!(A, B, C, D, E, F, G, H, I);
impl_resolvable_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_resolvable_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_resolvable_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);

// =============================================================================
// ServiceProvider Trait - Auto-registration
// =============================================================================

/// Extension trait for containers to auto-register services.
pub trait ServiceProvider {
    /// Register a service using its `Service` implementation.
    ///
    /// The service will be created lazily on first access, with dependencies
    /// resolved from the container.
    ///
    /// # Panics
    ///
    /// The created factory will panic at runtime if dependencies are missing.
    /// For compile-time safety, use the typed builder API.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dependency_injector::{Container, verified::{Service, ServiceProvider}};
    ///
    /// #[derive(Clone)]
    /// struct MyService;
    ///
    /// impl Service for MyService {
    ///     type Dependencies = ();
    ///     fn create(_: ()) -> Self { MyService }
    /// }
    ///
    /// let container = Container::new();
    /// container.provide::<MyService>();
    ///
    /// let service = container.get::<MyService>().unwrap();
    /// ```
    fn provide<T: Service>(&self);

    /// Register a service as a singleton with pre-resolved dependencies.
    ///
    /// Dependencies are resolved immediately, not lazily.
    ///
    /// # Returns
    ///
    /// `true` if all dependencies were resolved and the service was registered,
    /// `false` if any dependency was missing.
    fn provide_singleton<T: Service>(&self) -> bool;

    /// Register a service as transient.
    ///
    /// A new instance is created on every resolution.
    fn provide_transient<T: Service>(&self);
}

impl ServiceProvider for Container {
    #[inline]
    fn provide<T: Service>(&self) {
        let container = self.clone();
        self.lazy(move || {
            let deps = T::Dependencies::resolve(&container)
                .expect("Failed to resolve dependencies for service");
            T::create(deps)
        });
    }

    #[inline]
    fn provide_singleton<T: Service>(&self) -> bool {
        if let Some(deps) = T::Dependencies::resolve(self) {
            self.singleton(T::create(deps));
            true
        } else {
            false
        }
    }

    #[inline]
    fn provide_transient<T: Service>(&self) {
        let container = self.clone();
        self.transient(move || {
            let deps = T::Dependencies::resolve(&container)
                .expect("Failed to resolve dependencies for transient service");
            T::create(deps)
        });
    }
}

// =============================================================================
// ServiceModule - Group related services
// =============================================================================

/// A module that groups related service registrations.
///
/// # Example
///
/// ```rust
/// use dependency_injector::{Container, verified::{Service, ServiceModule, ServiceProvider}};
///
/// #[derive(Clone)]
/// struct Database;
///
/// impl Service for Database {
///     type Dependencies = ();
///     fn create(_: ()) -> Self { Database }
/// }
///
/// #[derive(Clone)]
/// struct Cache;
///
/// impl Service for Cache {
///     type Dependencies = ();
///     fn create(_: ()) -> Self { Cache }
/// }
///
/// struct DataModule;
///
/// impl ServiceModule for DataModule {
///     fn register(container: &Container) {
///         container.provide::<Database>();
///         container.provide::<Cache>();
///     }
/// }
///
/// let container = Container::new();
/// DataModule::register(&container);
///
/// assert!(container.contains::<Database>());
/// assert!(container.contains::<Cache>());
/// ```
pub trait ServiceModule {
    /// Register all services in this module.
    fn register(container: &Container);
}

// =============================================================================
// Dependency Graph Helpers
// =============================================================================

/// Trait for extracting dependency type information.
///
/// This is mainly useful for debugging and visualization.
pub trait DependencyInfo {
    /// Get the type names of all dependencies.
    fn dependency_names() -> Vec<&'static str>;
}

impl DependencyInfo for () {
    fn dependency_names() -> Vec<&'static str> {
        vec![]
    }
}

impl<T: Injectable> DependencyInfo for Arc<T> {
    fn dependency_names() -> Vec<&'static str> {
        vec![std::any::type_name::<T>()]
    }
}

impl<T: Injectable> DependencyInfo for Option<Arc<T>> {
    fn dependency_names() -> Vec<&'static str> {
        vec![std::any::type_name::<T>()]
    }
}

// Tuple implementations for DependencyInfo
macro_rules! impl_dependency_info_tuple {
    ($($T:ident),+) => {
        impl<$($T: Injectable),+> DependencyInfo for ($(Arc<$T>,)+) {
            fn dependency_names() -> Vec<&'static str> {
                vec![$(std::any::type_name::<$T>()),+]
            }
        }
    };
}

impl_dependency_info_tuple!(A, B);
impl_dependency_info_tuple!(A, B, C);
impl_dependency_info_tuple!(A, B, C, D);
impl_dependency_info_tuple!(A, B, C, D, E);
impl_dependency_info_tuple!(A, B, C, D, E, F);

// =============================================================================
// Compile-Time Cycle Detection (Documentation Only)
// =============================================================================

// Note: Full compile-time cycle detection requires either:
// 1. A procedural macro that analyzes the full dependency graph
// 2. Unstable Rust features (specialization, const generics)
//
// The current approach provides partial protection:
// - The `Service` trait requires explicit dependency declaration
// - The `TypedBuilder::with_dependencies` method verifies deps exist
// - Runtime errors are caught when resolving missing dependencies
//
// For complete compile-time cycle detection, consider:
// - Using the `#[derive(Service)]` macro which can analyze dependencies
// - Using the typed builder API which tracks registrations
//
// Future: When Rust's type system supports it, we can add full cycle detection.

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Config {
        debug: bool,
    }

    impl Service for Config {
        type Dependencies = ();

        fn create(_: ()) -> Self {
            Config { debug: true }
        }
    }

    #[derive(Clone)]
    struct Database {
        url: String,
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

    #[derive(Clone)]
    struct Cache {
        size: usize,
    }

    impl Service for Cache {
        type Dependencies = ();

        fn create(_: ()) -> Self {
            Cache { size: 1024 }
        }
    }

    #[derive(Clone)]
    struct UserRepository {
        db: Arc<Database>,
        cache: Arc<Cache>,
    }

    impl Service for UserRepository {
        type Dependencies = (Arc<Database>, Arc<Cache>);

        fn create((db, cache): (Arc<Database>, Arc<Cache>)) -> Self {
            UserRepository { db, cache }
        }
    }

    #[derive(Clone)]
    struct MixedDeps {
        db: Arc<Database>,
        cache: Option<Arc<Cache>>,
    }

    impl Service for MixedDeps {
        type Dependencies = (Arc<Database>, Option<Arc<Cache>>);

        fn create((db, cache): (Arc<Database>, Option<Arc<Cache>>)) -> Self {
            MixedDeps { db, cache }
        }
    }

    #[test]
    fn test_service_no_deps() {
        let container = Container::new();
        container.provide::<Config>();

        let config = container.get::<Config>().unwrap();
        assert!(config.debug);
    }

    #[test]
    fn test_service_single_dep() {
        let container = Container::new();
        container.provide::<Config>();
        container.provide::<Database>();

        let db = container.get::<Database>().unwrap();
        assert_eq!(db.url, "debug://localhost");
    }

    #[test]
    fn test_service_multiple_deps() {
        let container = Container::new();
        container.provide::<Config>();
        container.provide::<Database>();
        container.provide::<Cache>();
        container.provide::<UserRepository>();

        let repo = container.get::<UserRepository>().unwrap();
        assert_eq!(repo.db.url, "debug://localhost");
        assert_eq!(repo.cache.size, 1024);
    }

    #[test]
    fn test_provide_singleton() {
        let container = Container::new();
        container.provide::<Config>();

        // Should succeed
        let result = container.provide_singleton::<Database>();
        assert!(result);

        let db = container.get::<Database>().unwrap();
        assert_eq!(db.url, "debug://localhost");
    }

    #[test]
    fn test_provide_singleton_missing_dep() {
        let container = Container::new();

        // Should fail - Config not registered
        let result = container.provide_singleton::<Database>();
        assert!(!result);
    }

    #[test]
    fn test_provide_transient() {
        use std::sync::atomic::{AtomicU32, Ordering};

        static COUNTER: AtomicU32 = AtomicU32::new(0);

        #[derive(Clone)]
        struct Counter(u32);

        impl Service for Counter {
            type Dependencies = ();

            fn create(_: ()) -> Self {
                Counter(COUNTER.fetch_add(1, Ordering::SeqCst))
            }
        }

        let container = Container::new();
        container.provide_transient::<Counter>();

        let c1 = container.get::<Counter>().unwrap();
        let c2 = container.get::<Counter>().unwrap();

        assert_ne!(c1.0, c2.0);
    }

    #[test]
    fn test_optional_dependency() {
        #[derive(Clone)]
        struct OptionalCache;

        #[derive(Clone)]
        struct ServiceWithOptional {
            cache: Option<Arc<OptionalCache>>,
        }

        impl Service for ServiceWithOptional {
            type Dependencies = Option<Arc<OptionalCache>>;

            fn create(cache: Option<Arc<OptionalCache>>) -> Self {
                ServiceWithOptional { cache }
            }
        }

        let container = Container::new();
        container.provide::<ServiceWithOptional>();

        let svc = container.get::<ServiceWithOptional>().unwrap();
        assert!(svc.cache.is_none());

        // Now register the optional dep
        let container2 = Container::new();
        container2.singleton(OptionalCache);
        container2.provide::<ServiceWithOptional>();

        let svc2 = container2.get::<ServiceWithOptional>().unwrap();
        assert!(svc2.cache.is_some());
    }

    #[test]
    fn test_mixed_tuple_both_registered() {
        let container = Container::new();
        container.provide::<Config>();
        container.provide::<Database>();
        container.provide::<Cache>();
        container.provide::<MixedDeps>();

        let svc = container.get::<MixedDeps>().unwrap();
        assert_eq!(svc.db.url, "debug://localhost");
        assert!(svc.cache.is_some());
    }

    #[test]
    fn test_mixed_tuple_optional_missing() {
        let container = Container::new();
        container.provide::<Config>();
        container.provide::<Database>();
        // Cache not registered - optional dep resolves to None
        container.provide::<MixedDeps>();

        let svc = container.get::<MixedDeps>().unwrap();
        assert_eq!(svc.db.url, "debug://localhost");
        assert!(svc.cache.is_none());
    }

    #[test]
    fn test_mixed_tuple_required_missing() {
        let container = Container::new();
        // Database not registered - required dep missing, resolution fails
        assert!(<(Arc<Database>, Option<Arc<Cache>>) as Resolvable>::resolve(&container).is_none());
        assert!(!container.provide_singleton::<MixedDeps>());
    }

    #[test]
    fn test_dependency_info() {
        assert_eq!(
            <() as DependencyInfo>::dependency_names(),
            Vec::<&str>::new()
        );
        assert_eq!(
            <Arc<Config> as DependencyInfo>::dependency_names(),
            vec!["dependency_injector::verified::tests::Config"]
        );
        assert_eq!(
            <(Arc<Database>, Arc<Cache>) as DependencyInfo>::dependency_names().len(),
            2
        );
    }

    #[test]
    fn test_service_module() {
        struct TestModule;

        impl ServiceModule for TestModule {
            fn register(container: &Container) {
                container.provide::<Config>();
                container.provide::<Cache>();
            }
        }

        let container = Container::new();
        TestModule::register(&container);

        assert!(container.contains::<Config>());
        assert!(container.contains::<Cache>());
    }
    #[test]
    fn test_all_arc_tuple_trait_call_inference() {
        // Pins the annotated-binding trait-call form: the element-wise
        // generalization of the tuple impls must keep this resolving
        // without a turbofish on the impl.
        let container = Container::new();
        container.singleton(Config { debug: true });
        assert!(container.provide_singleton::<Database>());
        assert!(container.provide_singleton::<Cache>());

        let resolved: Option<(Arc<Database>, Arc<Cache>)> = Resolvable::resolve(&container);
        assert!(resolved.is_some());
    }
}

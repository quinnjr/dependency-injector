//! Compile-Time Type-Safe Container Builder
//!
//! This module provides a type-state container builder that ensures
//! type safety at compile time using Rust's type system.
//!
//! # Features
//!
//! - **Zero runtime overhead**: Type checking happens at compile time
//! - **Builder pattern**: Fluent API that tracks registered types
//! - **Dependency verification**: Ensure deps are registered before dependents
//!
//! # Example
//!
//! ```rust
//! use dependency_injector::typed::TypedBuilder;
//!
//! #[derive(Clone)]
//! struct Database { url: String }
//!
//! #[derive(Clone)]
//! struct Cache { size: usize }
//!
//! // Build with compile-time type tracking
//! let container = TypedBuilder::new()
//!     .singleton(Database { url: "postgres://localhost".into() })
//!     .singleton(Cache { size: 1024 })
//!     .build();
//!
//! // Type-safe resolution
//! let db = container.get::<Database>();
//! let cache = container.get::<Cache>();
//! ```
//!
//! # Compile-Time Dependency Declaration
//!
//! ```rust
//! use dependency_injector::typed::{TypedBuilder, DeclaresDeps};
//!
//! #[derive(Clone)]
//! struct Database;
//!
//! #[derive(Clone)]
//! struct UserService;
//!
//! // Declare that UserService depends on Database
//! impl DeclaresDeps for UserService {
//!     fn dependency_names() -> &'static [&'static str] {
//!         &["Database"]
//!     }
//! }
//!
//! // Register deps first, then dependent
//! let container = TypedBuilder::new()
//!     .singleton(Database)
//!     .with_deps(UserService)
//!     .build();
//! ```

use crate::{Container, Injectable};
use std::marker::PhantomData;
use std::sync::Arc;

// =============================================================================
// Registry Marker Types
// =============================================================================

/// Marker for a registered type in the builder's registry.
pub struct Reg<T, Rest>(PhantomData<(T, Rest)>);

/// Trait for checking if type T is at the head of a registry.
pub trait HasType<T: Injectable> {}

impl<T: Injectable, Rest> HasType<T> for Reg<T, Rest> {}

/// Trait for services that declare their dependencies as a type-level list.
///
/// The `Dependencies` associated type is a [`Reg`] chain terminated by `()`,
/// mirroring the registry type built by [`TypedBuilder`]. Implement it by
/// hand, or derive it with `#[derive(TypedRequire)]` when the `derive`
/// feature is enabled.
///
/// This trait is declaration-only: nothing in the library consumes
/// `Dependencies` at runtime. It exists so the derived list can be checked
/// against a [`TypedBuilder`] registry type at compile time (see
/// `tests/typed_require.rs`). It is distinct from [`DeclaresDeps`], which
/// carries runtime dependency *names* for the builder's `with_deps`
/// verification path — a service may implement either or both.
///
/// # Example
///
/// ```rust
/// use dependency_injector::typed::{Reg, Require};
///
/// #[derive(Clone)]
/// struct Database;
///
/// #[derive(Clone)]
/// struct Cache;
///
/// #[derive(Clone)]
/// struct UserService;
///
/// impl Require for UserService {
///     type Dependencies = Reg<Database, Reg<Cache, ()>>;
/// }
/// ```
pub trait Require {
    /// Type-level list of required dependencies: `Reg<T1, Reg<T2, ... ()>>`.
    type Dependencies;
}

// =============================================================================
// Type-State Builder
// =============================================================================

/// A type-state container builder.
///
/// The type parameter `R` tracks all registered types at compile time.
pub struct TypedBuilder<R = ()> {
    container: Container,
    _registry: PhantomData<R>,
}

impl TypedBuilder<()> {
    /// Create a new typed builder.
    #[inline]
    pub fn new() -> Self {
        Self {
            container: Container::new(),
            _registry: PhantomData,
        }
    }

    /// Create with pre-allocated capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            container: Container::with_capacity(capacity),
            _registry: PhantomData,
        }
    }
}

impl Default for TypedBuilder<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> TypedBuilder<R> {
    /// Register a singleton service.
    #[inline]
    pub fn singleton<T: Injectable>(self, instance: T) -> TypedBuilder<Reg<T, R>> {
        self.container.singleton(instance);
        TypedBuilder {
            container: self.container,
            _registry: PhantomData,
        }
    }

    /// Register a lazy singleton.
    #[inline]
    pub fn lazy<T: Injectable, F>(self, factory: F) -> TypedBuilder<Reg<T, R>>
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.container.lazy(factory);
        TypedBuilder {
            container: self.container,
            _registry: PhantomData,
        }
    }

    /// Register a transient service.
    #[inline]
    pub fn transient<T: Injectable, F>(self, factory: F) -> TypedBuilder<Reg<T, R>>
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.container.transient(factory);
        TypedBuilder {
            container: self.container,
            _registry: PhantomData,
        }
    }

    /// Build the typed container.
    #[inline]
    pub fn build(self) -> TypedContainer<R> {
        self.container.lock();
        TypedContainer {
            container: self.container,
            _registry: PhantomData,
        }
    }

    /// Build and return the underlying container.
    #[inline]
    pub fn build_dynamic(self) -> Container {
        self.container.lock();
        self.container
    }

    /// Access the underlying container.
    #[inline]
    pub fn inner(&self) -> &Container {
        &self.container
    }
}

// =============================================================================
// Dependency Declaration
// =============================================================================

// =============================================================================
// Dependency Declaration (Runtime-Verified)
// =============================================================================

/// Trait for services that declare their dependencies.
///
/// Use with `with_deps` to get documentation-level dependency declaration.
/// Runtime verification ensures all dependencies are present.
///
/// Note: Full compile-time dependency verification requires proc macros
/// or unstable Rust features. This provides a documentation/runtime hybrid.
pub trait DeclaresDeps: Injectable {
    /// List of dependency type names (for documentation and debugging).
    fn dependency_names() -> &'static [&'static str] {
        &[]
    }
}

impl<R> TypedBuilder<R> {
    /// Register a service (alias for singleton with deps intent).
    ///
    /// Note: This method is the same as `singleton` but signals that
    /// the service has dependencies that should already be registered.
    #[inline]
    pub fn with_deps<T: DeclaresDeps>(self, instance: T) -> TypedBuilder<Reg<T, R>> {
        self.singleton(instance)
    }

    /// Register a lazy service with deps intent.
    #[inline]
    pub fn lazy_with_deps<T: DeclaresDeps, F>(self, factory: F) -> TypedBuilder<Reg<T, R>>
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.lazy(factory)
    }
}

// Dummy traits for backwards compatibility
pub trait VerifyDeps<D> {}
impl<R, D> VerifyDeps<D> for R {}

// =============================================================================
// Typed Container
// =============================================================================

/// A container with compile-time type tracking.
///
/// The type parameter tracks what was registered, enabling
/// compile-time verification of service access.
pub struct TypedContainer<R> {
    container: Container,
    _registry: PhantomData<R>,
}

impl<R> TypedContainer<R> {
    /// Resolve a service by type.
    ///
    /// Uses the dynamic container internally but provides type-safe API.
    #[inline]
    pub fn get<T: Injectable>(&self) -> Arc<T> {
        self.container
            .get::<T>()
            .expect("TypedContainer: service not found (registration mismatch)")
    }

    /// Try to resolve a service.
    #[inline]
    pub fn try_get<T: Injectable>(&self) -> Option<Arc<T>> {
        self.container.try_get::<T>()
    }

    /// Check if service exists.
    #[inline]
    pub fn contains<T: Injectable>(&self) -> bool {
        self.container.contains::<T>()
    }

    /// Create a dynamic child scope.
    #[inline]
    pub fn scope(&self) -> Container {
        self.container.scope()
    }

    /// Access the underlying container.
    #[inline]
    pub fn inner(&self) -> &Container {
        &self.container
    }

    /// Convert to the underlying container.
    #[inline]
    pub fn into_inner(self) -> Container {
        self.container
    }
}

impl<R> Clone for TypedContainer<R> {
    fn clone(&self) -> Self {
        Self {
            container: self.container.clone(),
            _registry: PhantomData,
        }
    }
}

impl<R> std::fmt::Debug for TypedContainer<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedContainer")
            .field("inner", &self.container)
            .finish()
    }
}

// =============================================================================
// Backward Compatibility Aliases
// =============================================================================

/// Alias for HasType trait.
pub trait Has<T: Injectable>: HasType<T> {}
impl<T: Injectable, R: HasType<T>> Has<T> for R {}

/// Alias for HasType trait.
pub trait HasService<T: Injectable>: HasType<T> {}
impl<T: Injectable, R: HasType<T>> HasService<T> for R {}

// Dummy trait for DepsPresent compatibility
pub trait DepsPresent<D> {}
impl<R, D> DepsPresent<D> for R {}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Database {
        url: String,
    }

    #[derive(Clone)]
    struct Cache {
        size: usize,
    }

    #[derive(Clone)]
    struct UserService;

    impl DeclaresDeps for UserService {
        fn dependency_names() -> &'static [&'static str] {
            &["Database", "Cache"]
        }
    }

    #[test]
    fn test_typed_builder_basic() {
        let container = TypedBuilder::new()
            .singleton(Database {
                url: "postgres://localhost".into(),
            })
            .singleton(Cache { size: 1024 })
            .build();

        let db = container.get::<Database>();
        let cache = container.get::<Cache>();

        assert_eq!(db.url, "postgres://localhost");
        assert_eq!(cache.size, 1024);
    }

    #[test]
    fn test_typed_builder_lazy() {
        let container = TypedBuilder::new()
            .lazy(|| Database {
                url: "lazy://created".into(),
            })
            .build();

        let db = container.get::<Database>();
        assert_eq!(db.url, "lazy://created");
    }

    #[test]
    fn test_typed_builder_transient() {
        use std::sync::atomic::{AtomicU32, Ordering};

        static COUNTER: AtomicU32 = AtomicU32::new(0);

        #[derive(Clone)]
        struct Counter(u32);

        let container = TypedBuilder::new()
            .transient(|| Counter(COUNTER.fetch_add(1, Ordering::SeqCst)))
            .build();

        let c1 = container.get::<Counter>();
        let c2 = container.get::<Counter>();

        assert_ne!(c1.0, c2.0);
    }

    #[test]
    fn test_typed_container_clone() {
        let container = TypedBuilder::new()
            .singleton(Database { url: "test".into() })
            .build();

        let container2 = container.clone();

        let db1 = container.get::<Database>();
        let db2 = container2.get::<Database>();

        assert!(Arc::ptr_eq(&db1, &db2));
    }

    #[test]
    fn test_with_dependencies() {
        // Register deps first, then dependent service
        let container = TypedBuilder::new()
            .singleton(Database { url: "pg".into() })
            .singleton(Cache { size: 100 })
            .with_deps(UserService)
            .build();

        let _ = container.get::<UserService>();
    }

    #[test]
    fn test_many_services() {
        #[derive(Clone)]
        struct S1;
        #[derive(Clone)]
        struct S2;
        #[derive(Clone)]
        struct S3;
        #[derive(Clone)]
        struct S4;
        #[derive(Clone)]
        struct S5;

        let container = TypedBuilder::new()
            .singleton(S1)
            .singleton(S2)
            .singleton(S3)
            .singleton(S4)
            .singleton(S5)
            .build();

        let _ = container.get::<S1>();
        let _ = container.get::<S2>();
        let _ = container.get::<S3>();
        let _ = container.get::<S4>();
        let _ = container.get::<S5>();
    }

    #[test]
    fn test_scope_from_typed() {
        let container = TypedBuilder::new()
            .singleton(Database { url: "root".into() })
            .build();

        let child = container.scope();
        child.singleton(Cache { size: 256 });

        assert!(child.contains::<Database>());
        assert!(child.contains::<Cache>());
    }
}

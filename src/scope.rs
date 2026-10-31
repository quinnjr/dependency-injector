//! Scoped container support
//!
//! Provides utilities for working with scoped service lifetimes.

use crate::{Container, Injectable, Result};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "logging")]
use tracing::debug;

/// Unique scope identifier.
///
/// Each scope gets a unique ID for tracking and debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Scope(u64);

impl Scope {
    /// Generate a new unique scope ID.
    #[inline]
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Get the raw ID value.
    #[inline]
    pub fn id(&self) -> u64 {
        self.0
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scope-{}", self.0)
    }
}

/// A container with an associated scope identifier.
///
/// Useful for request-scoped or session-scoped services where you need
/// to track the scope identity.
///
/// # Examples
///
/// ```rust
/// use dependency_injector::{Container, ScopedContainer};
///
/// #[derive(Clone)]
/// struct RequestContext {
///     request_id: String,
/// }
///
/// let root = Container::new();
///
/// // Create a request-scoped container
/// let request = ScopedContainer::from_parent(&root);
/// request.singleton(RequestContext {
///     request_id: "req-123".to_string(),
/// });
///
/// let ctx = request.get::<RequestContext>().unwrap();
/// assert_eq!(ctx.request_id, "req-123");
/// ```
pub struct ScopedContainer {
    /// The underlying container
    container: Container,
    /// Scope identifier
    scope: Scope,
}

impl ScopedContainer {
    /// Create a new scoped container with no parent.
    #[inline]
    pub fn new() -> Self {
        let scope = Scope::new();

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            scope_id = scope.id(),
            "Creating new root ScopedContainer"
        );

        Self {
            container: Container::new(),
            scope,
        }
    }

    /// Create a scoped container from a parent container.
    #[inline]
    pub fn from_parent(parent: &Container) -> Self {
        let scope = Scope::new();

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            scope_id = scope.id(),
            parent_depth = parent.depth(),
            "Creating ScopedContainer from parent Container"
        );

        Self {
            container: parent.scope(),
            scope,
        }
    }

    /// Create a scoped container from another scoped container.
    #[inline]
    pub fn from_scope(parent: &ScopedContainer) -> Self {
        let scope = Scope::new();

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            scope_id = scope.id(),
            parent_scope_id = parent.scope.id(),
            "Creating child ScopedContainer from parent ScopedContainer"
        );

        Self {
            container: parent.container.scope(),
            scope,
        }
    }

    /// Get the scope identifier.
    #[inline]
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Register a singleton in this scope.
    #[inline]
    pub fn singleton<T: Injectable>(&self, instance: T) {
        self.container.singleton(instance);
    }

    /// Register a lazy singleton in this scope.
    #[inline]
    pub fn lazy<T: Injectable, F>(&self, factory: F)
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.container.lazy(factory);
    }

    /// Register a transient service in this scope.
    ///
    /// See [`crate::Container::transient`] for what the factory must not do.
    #[inline]
    pub fn transient<T: Injectable, F>(&self, factory: F)
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.container.transient(factory);
    }

    /// Alias for singleton - register an instance.
    #[inline]
    pub fn register<T: Injectable>(&self, instance: T) {
        self.container.register(instance);
    }

    /// Register using a factory.
    #[inline]
    pub fn register_factory<T: Injectable, F>(&self, factory: F)
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.container.register_factory(factory);
    }

    /// Resolve a service from this scope or parent scopes.
    #[inline]
    pub fn get<T: Injectable>(&self) -> Result<Arc<T>> {
        self.container.get::<T>()
    }

    /// Alias for get.
    #[inline]
    pub fn resolve<T: Injectable>(&self) -> Result<Arc<T>> {
        self.get::<T>()
    }

    /// Try to resolve a service, returning None if not found.
    #[inline]
    pub fn try_get<T: Injectable>(&self) -> Option<Arc<T>> {
        self.container.try_get::<T>()
    }

    /// Alias for try_get.
    #[inline]
    pub fn try_resolve<T: Injectable>(&self) -> Option<Arc<T>> {
        self.try_get::<T>()
    }

    /// Check if a service exists in this scope or parent scopes.
    #[inline]
    pub fn contains<T: Injectable>(&self) -> bool {
        self.container.contains::<T>()
    }

    /// Alias for contains.
    #[inline]
    pub fn has<T: Injectable>(&self) -> bool {
        self.contains::<T>()
    }

    /// Get the underlying container.
    #[inline]
    pub fn container(&self) -> &Container {
        &self.container
    }

    /// Get the underlying container mutably.
    #[inline]
    pub fn container_mut(&mut self) -> &mut Container {
        &mut self.container
    }

    /// Get the scope depth.
    #[inline]
    pub fn depth(&self) -> u32 {
        self.container.depth()
    }
}

impl Default for ScopedContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ScopedContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedContainer")
            .field("scope", &self.scope)
            .field("container", &self.container)
            .finish()
    }
}

/// Builder for creating scoped containers with pre-configured services.
///
/// Useful when you have a standard set of services to register per-scope.
///
/// # Examples
///
/// ```rust
/// use dependency_injector::{Container, ScopeBuilder};
/// use std::sync::atomic::{AtomicU64, Ordering};
///
/// static COUNTER: AtomicU64 = AtomicU64::new(0);
///
/// #[derive(Clone)]
/// struct RequestId(u64);
///
/// let root = Container::new();
///
/// let builder = ScopeBuilder::new()
///     .with_transient(|| RequestId(COUNTER.fetch_add(1, Ordering::SeqCst)));
///
/// let scope1 = builder.build(&root);
/// let scope2 = builder.build(&root);
///
/// // Each scope gets its own services
/// ```
pub struct ScopeBuilder {
    #[allow(clippy::type_complexity)]
    factories: Vec<Box<dyn Fn(&Container) + Send + Sync>>,
}

impl ScopeBuilder {
    /// Create a new scope builder.
    #[inline]
    pub fn new() -> Self {
        Self {
            factories: Vec::new(),
        }
    }

    /// Add a singleton factory to run in each scope.
    pub fn with_singleton<T, F>(mut self, factory: F) -> Self
    where
        T: Injectable + Clone,
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.factories.push(Box::new(move |container| {
            container.singleton(factory());
        }));
        self
    }

    /// Add a lazy singleton factory.
    pub fn with_lazy<T, F>(mut self, factory: F) -> Self
    where
        T: Injectable,
        F: Fn() -> T + Send + Sync + Clone + 'static,
    {
        self.factories.push(Box::new(move |container| {
            let f = factory.clone();
            container.lazy(f);
        }));
        self
    }

    /// Add a transient factory.
    ///
    /// See [`crate::Container::transient`] for what the factory must not do.
    pub fn with_transient<T, F>(mut self, factory: F) -> Self
    where
        T: Injectable,
        F: Fn() -> T + Send + Sync + Clone + 'static,
    {
        self.factories.push(Box::new(move |container| {
            let f = factory.clone();
            container.transient(f);
        }));
        self
    }

    /// Build a scoped container with all registered services.
    pub fn build(&self, parent: &Container) -> ScopedContainer {
        let scoped = ScopedContainer::from_parent(parent);
        for factory in &self.factories {
            factory(&scoped.container);
        }
        scoped
    }
}

impl Default for ScopeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct GlobalService;

    #[derive(Clone)]
    struct RequestService {
        id: String,
    }

    #[test]
    fn test_scoped_container() {
        let root = Container::new();
        root.singleton(GlobalService);

        let scoped = ScopedContainer::from_parent(&root);
        scoped.singleton(RequestService { id: "req-1".into() });

        // Can access both
        assert!(scoped.contains::<GlobalService>());
        assert!(scoped.contains::<RequestService>());

        // Root doesn't have scoped service
        assert!(!root.contains::<RequestService>());
    }

    #[test]
    fn test_scope_ids_unique() {
        let s1 = Scope::new();
        let s2 = Scope::new();
        let s3 = Scope::new();

        assert_ne!(s1.id(), s2.id());
        assert_ne!(s2.id(), s3.id());
    }

    #[test]
    fn test_scope_builder() {
        let root = Container::new();
        root.singleton(GlobalService);

        let builder = ScopeBuilder::new().with_singleton(|| RequestService { id: "built".into() });

        let scoped = builder.build(&root);

        assert!(scoped.contains::<GlobalService>());
        assert!(scoped.contains::<RequestService>());

        let req = scoped.get::<RequestService>().unwrap();
        assert_eq!(req.id, "built");
    }

    #[test]
    fn test_scope_display() {
        let scope = Scope::new();
        let display = format!("{}", scope);
        assert!(display.starts_with("scope-"));
    }
}

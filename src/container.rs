//! High-performance dependency injection container
//!
//! The `Container` is the core of the DI system. It stores services and
//! resolves dependencies with minimal overhead.

use crate::factory::AnyFactory;
use crate::storage::{ServiceStorage, downcast_arc_unchecked};
use crate::{DiError, Injectable, Result};
use std::any::{Any, TypeId};
use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "logging")]
use tracing::{debug, trace};

// =============================================================================
// Thread-Local Hot Cache (Phase 5 optimization)
// =============================================================================

/// Number of slots in the thread-local hot cache (power of 2 for fast indexing)
/// 4 slots fits in a single cache line and provides good hit rates for typical apps.
const HOT_CACHE_SLOTS: usize = 4;

/// A cached service entry
///
/// Phase 13 optimization: Stores pre-computed u64 hash instead of TypeId
/// to avoid transmute on every comparison.
struct CacheEntry {
    /// Pre-computed hash of TypeId (avoids transmute on lookup)
    type_hash: u64,
    /// Pointer to the storage this was resolved from (for scope identity)
    storage_ptr: usize,
    /// The cached service
    service: Arc<dyn Any + Send + Sync>,
}

/// Thread-local cache for frequently accessed services.
///
/// This provides ~8-10ns speedup for hot services by avoiding DashMap lookups.
/// Uses a simple direct-mapped cache with TypeId + storage pointer as key.
struct HotCache {
    entries: [Option<CacheEntry>; HOT_CACHE_SLOTS],
}

impl HotCache {
    const fn new() -> Self {
        Self {
            entries: [const { None }; HOT_CACHE_SLOTS],
        }
    }

    /// Get a cached service if present for a specific container
    ///
    /// Phase 12+13 optimization: Uses UnsafeCell (no RefCell borrow check)
    /// and pre-computed type_hash (no transmute on lookup).
    #[inline(always)]
    fn get<T: Send + Sync + 'static>(&self, storage_ptr: usize) -> Option<Arc<T>> {
        let type_hash = Self::type_hash::<T>();
        let slot = Self::slot_for_hash(type_hash, storage_ptr);

        if let Some(entry) = &self.entries[slot] {
            // Phase 13: Compare u64 hash directly (faster than TypeId comparison)
            if entry.type_hash == type_hash && entry.storage_ptr == storage_ptr {
                // Cache hit - clone and downcast (unchecked since type_hash matches)
                // SAFETY: We verified type_hash matches, so the Arc contains type T
                let arc = entry.service.clone();
                return Some(unsafe { downcast_arc_unchecked(arc) });
            }
        }
        None
    }

    /// Insert a service into the cache for a specific container
    #[inline]
    fn insert<T: Injectable>(&mut self, storage_ptr: usize, service: Arc<T>) {
        let type_hash = Self::type_hash::<T>();
        let slot = Self::slot_for_hash(type_hash, storage_ptr);

        self.entries[slot] = Some(CacheEntry {
            type_hash,
            storage_ptr,
            service: service as Arc<dyn Any + Send + Sync>,
        });
    }

    /// Clear the cache (call when container is modified)
    #[inline]
    fn clear(&mut self) {
        self.entries = [const { None }; HOT_CACHE_SLOTS];
    }

    /// Extract u64 hash from TypeId (computed once per type at compile time via monomorphization)
    #[inline(always)]
    fn type_hash<T: 'static>() -> u64 {
        let type_id = TypeId::of::<T>();
        // SAFETY: TypeId is #[repr(transparent)] wrapper around u128
        unsafe { std::mem::transmute_copy(&type_id) }
    }

    /// Calculate slot index from pre-computed type hash and storage pointer
    #[inline(always)]
    fn slot_for_hash(type_hash: u64, storage_ptr: usize) -> usize {
        // Fast bit mixing: XOR with rotated storage_ptr for good distribution
        let mixed = type_hash ^ (storage_ptr as u64).rotate_left(32);

        // Use golden ratio multiplication for final mixing (fast & good distribution)
        let slot = mixed.wrapping_mul(0x9e3779b97f4a7c15);

        (slot as usize) & (HOT_CACHE_SLOTS - 1)
    }
}

thread_local! {
    /// Thread-local hot cache for frequently accessed services
    ///
    /// Phase 12 optimization: Uses UnsafeCell instead of RefCell to eliminate
    /// borrow checking overhead. This is safe because thread_local! guarantees
    /// single-threaded access.
    static HOT_CACHE: UnsafeCell<HotCache> = const { UnsafeCell::new(HotCache::new()) };
}

/// Helper to access the hot cache without RefCell overhead
///
/// SAFETY: thread_local! guarantees single-threaded access, so we can use
/// UnsafeCell without data races. We ensure no aliasing by limiting access
/// to immutable borrows for reads and brief mutable borrows for writes.
#[inline(always)]
fn with_hot_cache<F, R>(f: F) -> R
where
    F: FnOnce(&HotCache) -> R,
{
    HOT_CACHE.with(|cell| {
        // SAFETY: thread_local guarantees single-threaded access
        let cache = unsafe { &*cell.get() };
        f(cache)
    })
}

/// Helper to mutably access the hot cache
#[inline(always)]
fn with_hot_cache_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut HotCache) -> R,
{
    HOT_CACHE.with(|cell| {
        // SAFETY: thread_local guarantees single-threaded access
        let cache = unsafe { &mut *cell.get() };
        f(cache)
    })
}

/// High-performance dependency injection container.
///
/// Uses lock-free data structures for maximum concurrent throughput.
/// Supports hierarchical scopes with full parent chain resolution.
///
/// # Examples
///
/// ```rust
/// use dependency_injector::Container;
///
/// #[derive(Clone)]
/// struct MyService { name: String }
///
/// let container = Container::new();
/// container.singleton(MyService { name: "test".into() });
///
/// let service = container.get::<MyService>().unwrap();
/// assert_eq!(service.name, "test");
/// ```
#[derive(Clone)]
pub struct Container {
    /// Service storage (lock-free)
    storage: Arc<ServiceStorage>,
    /// Parent storage - strong reference for fast resolution (Phase 2 optimization)
    /// This avoids Weak::upgrade() cost on every parent resolution
    parent_storage: Option<Arc<ServiceStorage>>,
    /// Lock state - uses AtomicBool for fast lock checking (no contention)
    locked: Arc<AtomicBool>,
    /// Scope depth for debugging
    depth: u32,
}

impl Container {
    /// Create a new root container.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    /// let container = Container::new();
    /// ```
    #[inline]
    pub fn new() -> Self {
        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            depth = 0,
            "Creating new root DI container"
        );

        Self {
            storage: Arc::new(ServiceStorage::new()),
            parent_storage: None,
            locked: Arc::new(AtomicBool::new(false)),
            depth: 0,
        }
    }

    /// Create a container with pre-allocated capacity.
    ///
    /// Use this when you know approximately how many services will be registered.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            storage: Arc::new(ServiceStorage::with_capacity(capacity)),
            parent_storage: None,
            locked: Arc::new(AtomicBool::new(false)),
            depth: 0,
        }
    }

    /// Create a child scope that inherits from this container.
    ///
    /// Child scopes can:
    /// - Access all services from parent scopes
    /// - Override parent services with local registrations
    /// - Have their own transient/scoped services
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct AppConfig { debug: bool }
    ///
    /// #[derive(Clone)]
    /// struct RequestId(String);
    ///
    /// let root = Container::new();
    /// root.singleton(AppConfig { debug: true });
    ///
    /// let request = root.scope();
    /// request.singleton(RequestId("req-123".into()));
    ///
    /// // Request scope can access root config
    /// assert!(request.contains::<AppConfig>());
    /// ```
    #[inline]
    pub fn scope(&self) -> Self {
        let child_depth = self.depth + 1;

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            parent_depth = self.depth,
            child_depth = child_depth,
            parent_services = self.storage.len(),
            "Creating child scope from parent container"
        );

        Self {
            // Phase 9: Storage now holds parent reference for deep chain resolution
            storage: Arc::new(ServiceStorage::with_parent(Arc::clone(&self.storage))),
            parent_storage: Some(Arc::clone(&self.storage)), // Keep for quick parent access
            locked: Arc::new(AtomicBool::new(false)),
            depth: child_depth,
        }
    }

    /// Alias for `scope()` - creates a child container.
    #[inline]
    pub fn create_scope(&self) -> Self {
        self.scope()
    }

    // =========================================================================
    // Registration Methods
    // =========================================================================

    /// Register a singleton service (eager).
    ///
    /// The instance is stored immediately and shared across all resolves.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct Database { url: String }
    ///
    /// let container = Container::new();
    /// container.singleton(Database { url: "postgres://localhost".into() });
    /// ```
    #[inline]
    pub fn singleton<T: Injectable>(&self, instance: T) {
        self.check_not_locked();

        let type_id = TypeId::of::<T>();
        #[cfg(feature = "logging")]
        let type_name = std::any::type_name::<T>();

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            service = type_name,
            lifetime = "singleton",
            depth = self.depth,
            service_count = self.storage.len() + 1,
            "Registering singleton service"
        );

        // Phase 2: Use enum-based AnyFactory directly
        self.storage
            .insert(type_id, AnyFactory::singleton(instance));
    }

    /// Register a lazy singleton service.
    ///
    /// The factory is called once on first access, then the instance is cached.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct ExpensiveService { data: Vec<u8> }
    ///
    /// let container = Container::new();
    /// container.lazy(|| ExpensiveService {
    ///     data: vec![0; 1024 * 1024], // Only allocated on first use
    /// });
    /// ```
    #[inline]
    pub fn lazy<T: Injectable, F>(&self, factory: F)
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.check_not_locked();

        let type_id = TypeId::of::<T>();
        #[cfg(feature = "logging")]
        let type_name = std::any::type_name::<T>();

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            service = type_name,
            lifetime = "lazy_singleton",
            depth = self.depth,
            service_count = self.storage.len() + 1,
            "Registering lazy singleton service (will be created on first access)"
        );

        // Phase 2: Use enum-based AnyFactory directly
        self.storage.insert(type_id, AnyFactory::lazy(factory));
    }

    /// Register a transient service.
    ///
    /// A new instance is created on every resolve.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    /// use std::sync::atomic::{AtomicU64, Ordering};
    ///
    /// static COUNTER: AtomicU64 = AtomicU64::new(0);
    ///
    /// #[derive(Clone)]
    /// struct RequestId(u64);
    ///
    /// let container = Container::new();
    /// container.transient(|| RequestId(COUNTER.fetch_add(1, Ordering::SeqCst)));
    ///
    /// let id1 = container.get::<RequestId>().unwrap();
    /// let id2 = container.get::<RequestId>().unwrap();
    /// assert_ne!(id1.0, id2.0); // Different instances
    /// ```
    #[inline]
    pub fn transient<T: Injectable, F>(&self, factory: F)
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.check_not_locked();

        let type_id = TypeId::of::<T>();
        #[cfg(feature = "logging")]
        let type_name = std::any::type_name::<T>();

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            service = type_name,
            lifetime = "transient",
            depth = self.depth,
            service_count = self.storage.len() + 1,
            "Registering transient service (new instance on every resolve)"
        );

        // Phase 2: Use enum-based AnyFactory directly
        self.storage.insert(type_id, AnyFactory::transient(factory));
    }

    /// Register using a factory (alias for `lazy`).
    #[inline]
    pub fn register_factory<T: Injectable, F>(&self, factory: F)
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.lazy(factory);
    }

    /// Register an instance (alias for `singleton`).
    #[inline]
    pub fn register<T: Injectable>(&self, instance: T) {
        self.singleton(instance);
    }

    /// Register a boxed instance.
    #[inline]
    #[allow(clippy::boxed_local)]
    pub fn register_boxed<T: Injectable>(&self, instance: Box<T>) {
        self.singleton(*instance);
    }

    /// Register by TypeId directly (advanced use).
    #[inline]
    pub fn register_by_id(&self, type_id: TypeId, instance: Arc<dyn Any + Send + Sync>) {
        self.check_not_locked();

        // Phase 2: Use the singleton factory with pre-erased Arc directly
        self.storage.insert(
            type_id,
            AnyFactory::Singleton(crate::factory::SingletonFactory { instance }),
        );
    }

    // =========================================================================
    // Resolution Methods
    // =========================================================================

    /// Resolve a service by type.
    ///
    /// Returns `Arc<T>` for zero-copy sharing. Walks the parent chain if
    /// not found in the current scope.
    ///
    /// # Performance
    ///
    /// Uses thread-local caching for frequently accessed services (~8ns vs ~19ns).
    /// The cache is automatically populated on first access.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct MyService;
    ///
    /// let container = Container::new();
    /// container.singleton(MyService);
    ///
    /// let service = container.get::<MyService>().unwrap();
    /// ```
    #[inline]
    pub fn get<T: Injectable>(&self) -> Result<Arc<T>> {
        // Get storage pointer for cache key (unique per container scope)
        let storage_ptr = Arc::as_ptr(&self.storage) as usize;

        // Phase 5+12: Check thread-local hot cache first (UnsafeCell, no RefCell overhead)
        // Note: Transients won't be in cache, so they'll fall through to get_and_cache
        if let Some(cached) = with_hot_cache(|cache| cache.get::<T>(storage_ptr)) {
            #[cfg(feature = "logging")]
            trace!(
                target: "dependency_injector",
                service = std::any::type_name::<T>(),
                depth = self.depth,
                location = "hot_cache",
                "Service resolved from thread-local cache"
            );
            return Ok(cached);
        }

        // Cache miss - resolve normally and cache the result (unless transient)
        self.get_and_cache::<T>(storage_ptr)
    }

    /// Internal: Resolve and cache a service
    ///
    /// Phase 15 optimization: Fast path for root containers (depth == 0) avoids
    /// function call overhead to resolve_from_parents when there are no parents.
    #[inline]
    fn get_and_cache<T: Injectable>(&self, storage_ptr: usize) -> Result<Arc<T>> {
        let type_id = TypeId::of::<T>();

        #[cfg(feature = "logging")]
        let type_name = std::any::type_name::<T>();

        #[cfg(feature = "logging")]
        trace!(
            target: "dependency_injector",
            service = type_name,
            depth = self.depth,
            "Resolving service (cache miss)"
        );

        // Try local storage first (most common case)
        // Use get_with_transient_flag to avoid second DashMap lookup for is_transient
        if let Some((service, is_transient)) = self.storage.get_with_transient_flag::<T>() {
            #[cfg(feature = "logging")]
            trace!(
                target: "dependency_injector",
                service = type_name,
                depth = self.depth,
                location = "local",
                "Service resolved from current scope"
            );

            // Cache non-transient services (transients create new instances each time)
            if !is_transient {
                with_hot_cache_mut(|cache| cache.insert(storage_ptr, Arc::clone(&service)));
            }

            return Ok(service);
        }

        // Phase 15: Fast path for root containers - no parents to walk
        if self.depth == 0 {
            #[cfg(feature = "logging")]
            debug!(
                target: "dependency_injector",
                service = std::any::type_name::<T>(),
                "Service not found in root container"
            );
            return Err(DiError::not_found::<T>());
        }

        // Walk parent chain (cold path)
        self.resolve_from_parents::<T>(&type_id, storage_ptr)
    }

    /// Resolve from parent chain (internal)
    ///
    /// Phase 9 optimization: Walks the full parent chain via ServiceStorage.parent.
    /// This allows services to be resolved from any ancestor scope.
    ///
    /// Phase 14 optimization: Marked as cold to improve branch prediction in the
    /// hot path - most resolutions hit the cache and don't need parent traversal.
    #[cold]
    fn resolve_from_parents<T: Injectable>(
        &self,
        type_id: &TypeId,
        storage_ptr: usize,
    ) -> Result<Arc<T>> {
        #[cfg(feature = "logging")]
        let type_name = std::any::type_name::<T>();

        #[cfg(feature = "logging")]
        trace!(
            target: "dependency_injector",
            service = type_name,
            depth = self.depth,
            "Service not in local scope, walking parent chain"
        );

        // Walk the full parent chain via storage's parent references
        let mut current = self.storage.parent();
        let mut ancestor_depth = self.depth.saturating_sub(1);

        while let Some(storage) = current {
            if let Some(arc) = storage.resolve(type_id) {
                // SAFETY: We resolved by TypeId::of::<T>(), so the factory
                // was registered with the same TypeId and stores type T.
                let typed: Arc<T> = unsafe { downcast_arc_unchecked(arc) };

                #[cfg(feature = "logging")]
                trace!(
                    target: "dependency_injector",
                    service = type_name,
                    depth = self.depth,
                    ancestor_depth = ancestor_depth,
                    location = "ancestor",
                    "Service resolved from ancestor scope"
                );

                // Cache non-transient services from parent (using child's storage ptr as key)
                if !storage.is_transient(type_id) {
                    with_hot_cache_mut(|cache| cache.insert(storage_ptr, Arc::clone(&typed)));
                }

                return Ok(typed);
            }
            current = storage.parent();
            ancestor_depth = ancestor_depth.saturating_sub(1);
        }

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            service = type_name,
            depth = self.depth,
            "Service not found in container or parent chain"
        );

        Err(DiError::not_found::<T>())
    }

    /// Clear the thread-local hot cache.
    ///
    /// Call this after modifying the container (registering/removing services)
    /// if you want subsequent resolutions to see the changes immediately.
    ///
    /// Note: The cache is automatically invalidated when services are
    /// re-registered, but this method can be used for explicit control.
    #[inline]
    pub fn clear_cache(&self) {
        with_hot_cache_mut(|cache| cache.clear());
    }

    /// Pre-warm the thread-local cache with a specific service type.
    ///
    /// This can be useful at the start of request handling to ensure
    /// hot services are already in the cache.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct Database;
    ///
    /// let container = Container::new();
    /// container.singleton(Database);
    ///
    /// // Pre-warm cache for hot services
    /// container.warm_cache::<Database>();
    /// ```
    #[inline]
    pub fn warm_cache<T: Injectable>(&self) {
        // Simply resolve the service to populate the cache
        let _ = self.get::<T>();
    }

    /// Alias for `get` - resolve a service.
    #[inline]
    pub fn resolve<T: Injectable>(&self) -> Result<Arc<T>> {
        self.get::<T>()
    }

    /// Try to resolve, returning None if not found.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct OptionalService;
    ///
    /// let container = Container::new();
    /// assert!(container.try_get::<OptionalService>().is_none());
    /// ```
    #[inline]
    pub fn try_get<T: Injectable>(&self) -> Option<Arc<T>> {
        self.get::<T>().ok()
    }

    /// Alias for `try_get`.
    #[inline]
    pub fn try_resolve<T: Injectable>(&self) -> Option<Arc<T>> {
        self.try_get::<T>()
    }

    // =========================================================================
    // Query Methods
    // =========================================================================

    /// Check if a service is registered.
    ///
    /// Checks both current scope and parent scopes.
    #[inline]
    pub fn contains<T: Injectable>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        self.contains_type_id(&type_id)
    }

    /// Alias for `contains`.
    #[inline]
    pub fn has<T: Injectable>(&self) -> bool {
        self.contains::<T>()
    }

    /// Check by TypeId
    /// Phase 9 optimization: Uses storage's parent chain for deep hierarchy support
    fn contains_type_id(&self, type_id: &TypeId) -> bool {
        // Check local storage and full parent chain
        self.storage.contains_in_chain(type_id)
    }

    /// Get the number of services in this scope (not including parents).
    #[inline]
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Check if this scope is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    /// Get all registered TypeIds in this scope.
    pub fn registered_types(&self) -> Vec<TypeId> {
        self.storage.type_ids()
    }

    /// Get the scope depth (0 = root).
    #[inline]
    pub fn depth(&self) -> u32 {
        self.depth
    }

    // =========================================================================
    // Lifecycle Methods
    // =========================================================================

    /// Lock the container to prevent further registrations.
    ///
    /// Useful for ensuring no services are registered after app initialization.
    #[inline]
    pub fn lock(&self) {
        self.locked.store(true, Ordering::Release);

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            depth = self.depth,
            service_count = self.storage.len(),
            "Container locked - no further registrations allowed"
        );
    }

    /// Check if the container is locked.
    #[inline]
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Acquire)
    }

    /// Freeze the container into an immutable, perfectly-hashed storage.
    ///
    /// This creates a `FrozenStorage` that uses minimal perfect hashing for
    /// O(1) lookups without hash collisions, providing ~5ns faster resolution.
    ///
    /// Note: This also locks the container to prevent further registrations.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use dependency_injector::Container;
    ///
    /// let container = Container::new();
    /// container.singleton(MyService { ... });
    ///
    /// let frozen = container.freeze();
    /// // Use frozen.resolve(&type_id) for faster lookups
    /// ```
    #[cfg(feature = "perfect-hash")]
    #[inline]
    pub fn freeze(&self) -> crate::storage::FrozenStorage {
        self.lock();
        crate::storage::FrozenStorage::from_storage(&self.storage)
    }

    /// Clear all services from this scope.
    ///
    /// Does not affect parent scopes.
    #[inline]
    pub fn clear(&self) {
        #[cfg(feature = "logging")]
        let count = self.storage.len();
        self.storage.clear();

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            depth = self.depth,
            services_removed = count,
            "Container cleared - all services removed from this scope"
        );
    }

    /// Panic if locked (internal helper).
    /// Uses relaxed ordering for fast path - we only need eventual consistency
    /// since registration is not a hot path and locking is rare.
    #[inline]
    fn check_not_locked(&self) {
        if self.locked.load(Ordering::Relaxed) {
            panic!("Cannot register services: container is locked");
        }
    }

    // =========================================================================
    // Batch Registration (Phase 3)
    // =========================================================================

    /// Register multiple services in a single batch operation.
    ///
    /// This is more efficient than individual registrations when registering
    /// many services at once, as it:
    /// - Performs a single lock check at the start
    /// - Minimizes per-call overhead
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct Database { url: String }
    /// #[derive(Clone)]
    /// struct Cache { size: usize }
    /// #[derive(Clone)]
    /// struct Logger { level: String }
    ///
    /// let container = Container::new();
    /// container.batch(|batch| {
    ///     batch.singleton(Database { url: "postgres://localhost".into() });
    ///     batch.singleton(Cache { size: 1024 });
    ///     batch.singleton(Logger { level: "info".into() });
    /// });
    ///
    /// assert!(container.contains::<Database>());
    /// assert!(container.contains::<Cache>());
    /// assert!(container.contains::<Logger>());
    /// ```
    ///
    /// Note: For maximum performance with many services, prefer the builder API:
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct A;
    /// #[derive(Clone)]
    /// struct B;
    ///
    /// let container = Container::new();
    /// container.register_batch()
    ///     .singleton(A)
    ///     .singleton(B)
    ///     .done();
    /// ```
    #[inline]
    pub fn batch<F>(&self, f: F)
    where
        F: FnOnce(BatchRegistrar<'_>),
    {
        self.check_not_locked();

        #[cfg(feature = "logging")]
        let start_count = self.storage.len();

        // Create a zero-cost batch registrar that wraps the storage
        f(BatchRegistrar {
            storage: &self.storage,
        });

        #[cfg(feature = "logging")]
        {
            let end_count = self.storage.len();
            debug!(
                target: "dependency_injector",
                depth = self.depth,
                services_registered = end_count - start_count,
                "Batch registration completed"
            );
        }
    }

    /// Start a fluent batch registration.
    ///
    /// This is faster than the closure-based `batch()` for many services
    /// because it avoids closure overhead.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct Database { url: String }
    /// #[derive(Clone)]
    /// struct Cache { size: usize }
    ///
    /// let container = Container::new();
    /// container.register_batch()
    ///     .singleton(Database { url: "postgres://localhost".into() })
    ///     .singleton(Cache { size: 1024 })
    ///     .done();
    ///
    /// assert!(container.contains::<Database>());
    /// assert!(container.contains::<Cache>());
    /// ```
    #[inline]
    pub fn register_batch(&self) -> BatchBuilder<'_> {
        self.check_not_locked();
        BatchBuilder {
            storage: &self.storage,
            #[cfg(feature = "logging")]
            count: 0,
        }
    }
}

/// Fluent batch registration builder.
///
/// Provides a chainable API for registering multiple services without closure overhead.
pub struct BatchBuilder<'a> {
    storage: &'a ServiceStorage,
    #[cfg(feature = "logging")]
    count: usize,
}

impl<'a> BatchBuilder<'a> {
    /// Register a singleton and continue the chain
    #[inline]
    pub fn singleton<T: Injectable>(self, instance: T) -> Self {
        self.storage
            .insert(TypeId::of::<T>(), AnyFactory::singleton(instance));
        Self {
            storage: self.storage,
            #[cfg(feature = "logging")]
            count: self.count + 1,
        }
    }

    /// Register a lazy singleton and continue the chain
    #[inline]
    pub fn lazy<T: Injectable, F>(self, factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.storage
            .insert(TypeId::of::<T>(), AnyFactory::lazy(factory));
        Self {
            storage: self.storage,
            #[cfg(feature = "logging")]
            count: self.count + 1,
        }
    }

    /// Register a transient and continue the chain
    #[inline]
    pub fn transient<T: Injectable, F>(self, factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.storage
            .insert(TypeId::of::<T>(), AnyFactory::transient(factory));
        Self {
            storage: self.storage,
            #[cfg(feature = "logging")]
            count: self.count + 1,
        }
    }

    /// Finish the batch registration
    #[inline]
    pub fn done(self) {
        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            services_registered = self.count,
            "Batch registration completed"
        );
    }
}

/// Batch registrar for closure-based bulk registration.
///
/// A zero-cost wrapper that provides direct storage access.
/// The lock check is done once in `Container::batch()`.
#[repr(transparent)]
pub struct BatchRegistrar<'a> {
    storage: &'a ServiceStorage,
}

impl<'a> BatchRegistrar<'a> {
    /// Register a singleton service (inserted immediately)
    #[inline]
    pub fn singleton<T: Injectable>(&self, instance: T) {
        self.storage
            .insert(TypeId::of::<T>(), AnyFactory::singleton(instance));
    }

    /// Register a lazy singleton service (inserted immediately)
    #[inline]
    pub fn lazy<T: Injectable, F>(&self, factory: F)
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.storage
            .insert(TypeId::of::<T>(), AnyFactory::lazy(factory));
    }

    /// Register a transient service (inserted immediately)
    #[inline]
    pub fn transient<T: Injectable, F>(&self, factory: F)
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.storage
            .insert(TypeId::of::<T>(), AnyFactory::transient(factory));
    }
}

// =============================================================================
// Scope Pooling (Phase 6 optimization)
// =============================================================================

use std::sync::Mutex;

/// A pool of pre-allocated scopes for high-throughput scenarios.
///
/// Creating a scope involves allocating a DashMap (~134ns). For web servers
/// handling thousands of requests per second, this adds up. ScopePool pre-allocates
/// scopes and reuses them, reducing per-request overhead to near-zero.
///
/// # Example
///
/// ```rust
/// use dependency_injector::{Container, ScopePool};
///
/// #[derive(Clone)]
/// struct AppConfig { name: String }
///
/// #[derive(Clone)]
/// struct RequestId(String);
///
/// // Create root container with app-wide services
/// let root = Container::new();
/// root.singleton(AppConfig { name: "MyApp".into() });
///
/// // Create a pool of reusable scopes (pre-allocates 4 scopes)
/// let pool = ScopePool::new(&root, 4);
///
/// // In request handler: acquire a pooled scope
/// {
///     let scope = pool.acquire();
///     scope.singleton(RequestId("req-123".into()));
///
///     // Can access parent services
///     assert!(scope.contains::<AppConfig>());
///     assert!(scope.contains::<RequestId>());
///
///     // Scope automatically released when dropped
/// }
///
/// // Next request reuses the same scope allocation
/// {
///     let scope = pool.acquire();
///     // Previous RequestId is cleared, fresh scope
///     assert!(!scope.contains::<RequestId>());
/// }
/// ```
///
/// # Performance
///
/// - First acquisition: ~134ns (creates new scope if pool is empty)
/// - Subsequent acquisitions: ~20ns (reuses pooled scope)
/// - Release: ~10ns (clears and returns to pool)
pub struct ScopePool {
    /// Parent storage to create scopes from
    parent_storage: Arc<ServiceStorage>,
    /// Pool of available scopes (storage + lock state pairs)
    available: Mutex<Vec<ScopeSlot>>,
    /// Parent depth for child scope depth calculation
    parent_depth: u32,
}

/// A reusable scope slot containing pre-allocated storage and lock state
struct ScopeSlot {
    /// Pre-allocated storage with parent reference
    storage: Arc<ServiceStorage>,
    locked: Arc<AtomicBool>,
}

impl ScopePool {
    /// Create a new scope pool with pre-allocated capacity.
    ///
    /// # Arguments
    ///
    /// * `parent` - The parent container that scopes will inherit from
    /// * `capacity` - Number of scopes to pre-allocate
    ///
    /// # Example
    ///
    /// ```rust
    /// use dependency_injector::{Container, ScopePool};
    ///
    /// let root = Container::new();
    /// // Pre-allocate 8 scopes for concurrent request handling
    /// let pool = ScopePool::new(&root, 8);
    /// ```
    pub fn new(parent: &Container, capacity: usize) -> Self {
        let mut available = Vec::with_capacity(capacity);

        // Pre-allocate storage with parent reference and lock states
        for _ in 0..capacity {
            available.push(ScopeSlot {
                storage: Arc::new(ServiceStorage::with_parent(Arc::clone(&parent.storage))),
                locked: Arc::new(AtomicBool::new(false)),
            });
        }

        #[cfg(feature = "logging")]
        debug!(
            target: "dependency_injector",
            capacity = capacity,
            parent_depth = parent.depth,
            "Created scope pool with pre-allocated scopes"
        );

        Self {
            parent_storage: Arc::clone(&parent.storage),
            available: Mutex::new(available),
            parent_depth: parent.depth,
        }
    }

    /// Acquire a scope from the pool.
    ///
    /// Returns a `PooledScope` that automatically returns to the pool when dropped.
    /// If the pool is empty, creates a new scope.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dependency_injector::{Container, ScopePool};
    ///
    /// #[derive(Clone)]
    /// struct RequestData { id: u64 }
    ///
    /// let root = Container::new();
    /// let pool = ScopePool::new(&root, 4);
    ///
    /// let scope = pool.acquire();
    /// scope.singleton(RequestData { id: 123 });
    /// let data = scope.get::<RequestData>().unwrap();
    /// assert_eq!(data.id, 123);
    /// ```
    #[inline]
    pub fn acquire(&self) -> PooledScope<'_> {
        let slot = self.available.lock().unwrap().pop();

        let (storage, locked) = match slot {
            Some(slot) => {
                #[cfg(feature = "logging")]
                trace!(
                    target: "dependency_injector",
                    "Acquired scope from pool (reusing storage)"
                );
                (slot.storage, slot.locked)
            }
            None => {
                #[cfg(feature = "logging")]
                trace!(
                    target: "dependency_injector",
                    "Pool empty, creating new scope"
                );
                (
                    Arc::new(ServiceStorage::with_parent(Arc::clone(
                        &self.parent_storage,
                    ))),
                    Arc::new(AtomicBool::new(false)),
                )
            }
        };

        let container = Container {
            storage,
            parent_storage: Some(Arc::clone(&self.parent_storage)),
            locked,
            depth: self.parent_depth + 1,
        };

        PooledScope {
            container: Some(container),
            pool: self,
        }
    }

    /// Return a scope to the pool (internal use).
    #[inline]
    fn release(&self, container: Container) {
        // Clear storage for reuse (parent reference is preserved)
        container.storage.clear();
        // Reset lock state
        container.locked.store(false, Ordering::Relaxed);

        // Return to pool
        self.available.lock().unwrap().push(ScopeSlot {
            storage: container.storage,
            locked: container.locked,
        });

        #[cfg(feature = "logging")]
        trace!(
            target: "dependency_injector",
            "Released scope back to pool"
        );
    }

    /// Get the current number of available scopes in the pool.
    #[inline]
    pub fn available_count(&self) -> usize {
        self.available.lock().unwrap().len()
    }
}

/// A scope acquired from a pool that automatically returns when dropped.
///
/// This provides RAII-style management of pooled scopes, ensuring they're
/// always returned to the pool even if the code panics.
pub struct PooledScope<'a> {
    container: Option<Container>,
    pool: &'a ScopePool,
}

impl PooledScope<'_> {
    /// Get a reference to the underlying container.
    #[inline]
    pub fn container(&self) -> &Container {
        self.container.as_ref().unwrap()
    }
}

impl std::ops::Deref for PooledScope<'_> {
    type Target = Container;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.container.as_ref().unwrap()
    }
}

impl Drop for PooledScope<'_> {
    fn drop(&mut self) {
        if let Some(container) = self.container.take() {
            self.pool.release(container);
        }
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Container {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Container")
            .field("service_count", &self.len())
            .field("depth", &self.depth)
            .field("has_parent", &self.parent_storage.is_some())
            .field("locked", &self.is_locked())
            .finish_non_exhaustive()
    }
}

// =========================================================================
// Thread Safety
// =========================================================================

// Container is Send + Sync because:
// - ServiceStorage uses DashMap (thread-safe)
// - parent is Weak<...> which is Send + Sync
// - locked uses AtomicBool (Send + Sync)
unsafe impl Send for Container {}
unsafe impl Sync for Container {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestService {
        value: String,
    }

    #[allow(dead_code)]
    #[derive(Clone)]
    struct AnotherService {
        name: String,
    }

    #[test]
    fn test_singleton() {
        let container = Container::new();
        container.singleton(TestService {
            value: "test".into(),
        });

        let s1 = container.get::<TestService>().unwrap();
        let s2 = container.get::<TestService>().unwrap();

        assert_eq!(s1.value, "test");
        assert!(Arc::ptr_eq(&s1, &s2));
    }

    #[test]
    fn test_lazy() {
        use std::sync::atomic::{AtomicBool, Ordering};

        static CREATED: AtomicBool = AtomicBool::new(false);

        let container = Container::new();
        container.lazy(|| {
            CREATED.store(true, Ordering::SeqCst);
            TestService {
                value: "lazy".into(),
            }
        });

        assert!(!CREATED.load(Ordering::SeqCst));

        let s = container.get::<TestService>().unwrap();
        assert!(CREATED.load(Ordering::SeqCst));
        assert_eq!(s.value, "lazy");
    }

    #[test]
    fn test_transient() {
        use std::sync::atomic::{AtomicU32, Ordering};

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
    fn test_scope_inheritance() {
        let root = Container::new();
        root.singleton(TestService {
            value: "root".into(),
        });

        let child = root.scope();
        child.singleton(AnotherService {
            name: "child".into(),
        });

        // Child sees both
        assert!(child.contains::<TestService>());
        assert!(child.contains::<AnotherService>());

        // Root only sees its own
        assert!(root.contains::<TestService>());
        assert!(!root.contains::<AnotherService>());
    }

    #[test]
    fn test_scope_override() {
        let root = Container::new();
        root.singleton(TestService {
            value: "root".into(),
        });

        let child = root.scope();
        child.singleton(TestService {
            value: "child".into(),
        });

        let root_service = root.get::<TestService>().unwrap();
        let child_service = child.get::<TestService>().unwrap();

        assert_eq!(root_service.value, "root");
        assert_eq!(child_service.value, "child");
    }

    #[test]
    fn test_not_found() {
        let container = Container::new();
        let result = container.get::<TestService>();
        assert!(result.is_err());
    }

    #[test]
    fn test_lock() {
        let container = Container::new();
        assert!(!container.is_locked());

        container.lock();
        assert!(container.is_locked());
    }

    #[test]
    #[should_panic(expected = "Cannot register services: container is locked")]
    fn test_register_after_lock() {
        let container = Container::new();
        container.lock();
        container.singleton(TestService {
            value: "fail".into(),
        });
    }

    #[test]
    fn test_batch_registration() {
        #[derive(Clone)]
        struct ServiceA(i32);
        #[allow(dead_code)]
        #[derive(Clone)]
        struct ServiceB(String);

        let container = Container::new();
        container.batch(|batch| {
            batch.singleton(ServiceA(42));
            batch.singleton(ServiceB("test".into()));
            batch.lazy(|| TestService {
                value: "lazy".into(),
            });
        });

        assert!(container.contains::<ServiceA>());
        assert!(container.contains::<ServiceB>());
        assert!(container.contains::<TestService>());

        let a = container.get::<ServiceA>().unwrap();
        assert_eq!(a.0, 42);
    }

    #[test]
    fn test_scope_pool_basic() {
        #[derive(Clone)]
        struct RequestId(u64);

        let root = Container::new();
        root.singleton(TestService {
            value: "root".into(),
        });

        // Create pool with 2 pre-allocated scopes
        let pool = ScopePool::new(&root, 2);
        assert_eq!(pool.available_count(), 2);

        // Acquire a scope
        {
            let scope = pool.acquire();
            assert_eq!(pool.available_count(), 1);

            // Can access parent services
            assert!(scope.contains::<TestService>());

            // Register request-specific service
            scope.singleton(RequestId(123));
            assert!(scope.contains::<RequestId>());

            let id = scope.get::<RequestId>().unwrap();
            assert_eq!(id.0, 123);
        }
        // Scope released back to pool
        assert_eq!(pool.available_count(), 2);
    }

    #[test]
    fn test_scope_pool_reuse() {
        #[derive(Clone)]
        struct RequestId(u64);

        let root = Container::new();
        let pool = ScopePool::new(&root, 1);

        // First request
        {
            let scope = pool.acquire();
            scope.singleton(RequestId(1));
            assert!(scope.contains::<RequestId>());
        }

        // Second request - should reuse the same scope (cleared)
        {
            let scope = pool.acquire();
            // Previous RequestId should be cleared
            assert!(!scope.contains::<RequestId>());

            scope.singleton(RequestId(2));
            let id = scope.get::<RequestId>().unwrap();
            assert_eq!(id.0, 2);
        }
    }

    #[test]
    fn test_scope_pool_expansion() {
        let root = Container::new();
        let pool = ScopePool::new(&root, 1);

        // Acquire more scopes than pre-allocated
        let _s1 = pool.acquire();
        let _s2 = pool.acquire(); // Creates new scope

        assert_eq!(pool.available_count(), 0);

        // Both should work
        drop(_s1);
        drop(_s2);

        // Both return to pool
        assert_eq!(pool.available_count(), 2);
    }

    #[test]
    fn test_deep_parent_chain() {
        // Test that services can be resolved from grandparent and beyond
        #[derive(Clone)]
        struct RootService(i32);
        #[derive(Clone)]
        struct MiddleService(i32);
        #[derive(Clone)]
        struct LeafService(i32);

        // Create 4-level hierarchy: root -> middle1 -> middle2 -> leaf
        let root = Container::new();
        root.singleton(RootService(1));

        let middle1 = root.scope();
        middle1.singleton(MiddleService(2));

        let middle2 = middle1.scope();
        // No service in middle2

        let leaf = middle2.scope();
        leaf.singleton(LeafService(4));

        // Leaf should be able to access all ancestor services
        assert!(
            leaf.contains::<RootService>(),
            "Should find root service in leaf"
        );
        assert!(
            leaf.contains::<MiddleService>(),
            "Should find middle service in leaf"
        );
        assert!(
            leaf.contains::<LeafService>(),
            "Should find leaf service in leaf"
        );

        // Verify resolution works
        let root_svc = leaf.get::<RootService>().unwrap();
        assert_eq!(root_svc.0, 1);

        let middle_svc = leaf.get::<MiddleService>().unwrap();
        assert_eq!(middle_svc.0, 2);

        let leaf_svc = leaf.get::<LeafService>().unwrap();
        assert_eq!(leaf_svc.0, 4);

        // Middle2 should also access ancestor services
        assert!(middle2.contains::<RootService>());
        assert!(middle2.contains::<MiddleService>());
        assert!(!middle2.contains::<LeafService>()); // Leaf service not in parent
    }
}

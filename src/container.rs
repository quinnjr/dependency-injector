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
/// 4 slots keeps the cache compact (a few cache lines) and provides good hit
/// rates for typical apps.
const HOT_CACHE_SLOTS: usize = 4;
// `slot_for` keeps the top log2(HOT_CACHE_SLOTS) bits of a 64-bit product.
const _: () = assert!(HOT_CACHE_SLOTS.is_power_of_two() && HOT_CACHE_SLOTS > 1);

/// A cached service entry
///
/// The full `TypeId` is stored and compared exactly: slot collisions are
/// expected, so this check is what guarantees the unchecked downcast is sound.
struct CacheEntry {
    /// Exact type of the cached service
    type_id: TypeId,
    /// Pointer to the storage this was resolved from (for scope identity)
    storage_ptr: usize,
    /// Storage generation at the time of caching (for staleness detection).
    /// Generations are stamped by `ServiceStorage` on every mutation and are
    /// globally unique, which also defeats `Arc::as_ptr` ABA reuse.
    generation: u64,
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
    /// Phase 12 optimization: Uses UnsafeCell (no RefCell borrow check).
    ///
    /// The entry only hits if its generation matches the storage's current
    /// generation, so entries cached before a mutation are never returned.
    #[inline(always)]
    fn get<T: Send + Sync + 'static>(&self, storage_ptr: usize, generation: u64) -> Option<Arc<T>> {
        let type_id = TypeId::of::<T>();
        let slot = Self::slot_for::<T>(storage_ptr);

        if let Some(entry) = &self.entries[slot] {
            if entry.type_id == type_id
                && entry.storage_ptr == storage_ptr
                && entry.generation == generation
            {
                // Cache hit - clone and downcast (unchecked since TypeId matches)
                // SAFETY: entry.type_id == TypeId::of::<T>(), and `insert` is
                // generic over T and stores `TypeId::of::<T>()` alongside an
                // `Arc<T>`, so the Arc contains T.
                let arc = entry.service.clone();
                return Some(unsafe { downcast_arc_unchecked(arc) });
            }
        }
        None
    }

    /// Insert a service into the cache for a specific container.
    ///
    /// Returns the evicted entry instead of dropping it: dropping a service
    /// can run user `Drop` code that re-enters the cache, so the caller must
    /// drop it after the mutable borrow has ended.
    #[inline]
    #[must_use = "drop the evicted entry outside the hot cache borrow"]
    fn insert<T: Injectable>(
        &mut self,
        storage_ptr: usize,
        generation: u64,
        service: Arc<T>,
    ) -> Option<CacheEntry> {
        let slot = Self::slot_for::<T>(storage_ptr);

        self.entries[slot].replace(CacheEntry {
            type_id: TypeId::of::<T>(),
            storage_ptr,
            generation,
            service: service as Arc<dyn Any + Send + Sync>,
        })
    }

    /// Clear the cache, returning the old entries for the caller to drop
    /// outside the mutable borrow (see [`HotCache::insert`]).
    #[inline]
    #[must_use = "drop the evicted entries outside the hot cache borrow"]
    fn clear(&mut self) -> [Option<CacheEntry>; HOT_CACHE_SLOTS] {
        std::mem::replace(&mut self.entries, [const { None }; HOT_CACHE_SLOTS])
    }

    /// Hash a TypeId for slot selection only (never used as a type check).
    ///
    /// Goes through `TypeId: Hash` because `TypeId`'s layout is unspecified.
    #[inline(always)]
    fn type_hash<T: 'static>() -> u64 {
        ahash::RandomState::with_seeds(
            0x243f_6a88_85a3_08d3,
            0x1319_8a2e_0370_7344,
            0xa409_3822_299f_31d0,
            0x082e_fa98_ec4e_6c89,
        )
        .hash_one(TypeId::of::<T>())
    }

    /// Calculate slot index for type `T` in a given storage
    #[inline(always)]
    fn slot_for<T: 'static>(storage_ptr: usize) -> usize {
        // Fast bit mixing: XOR with rotated storage_ptr for good distribution
        let mixed = Self::type_hash::<T>() ^ (storage_ptr as u64).rotate_left(32);

        // Fibonacci hashing: multiply by the golden ratio and keep the TOP
        // bits, which depend on every input bit (the low bits of a product
        // depend only on the low bits of its operands).
        let slot = mixed.wrapping_mul(0x9e3779b97f4a7c15);

        (slot >> (u64::BITS - HOT_CACHE_SLOTS.trailing_zeros())) as usize
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

// Hot cache access. These three functions are the only code that borrows the
// `UnsafeCell<HotCache>`, and none of them runs user code while the borrow is
// live: lookups only clone an `Arc`, and insert/clear move evicted entries out
// and drop them after the borrow ends (a service's `Drop` may re-enter the
// cache). `try_with` makes access during thread-local teardown a cache miss
// instead of a panic, since services dropped by the cache's own destructor
// may still resolve other services.

/// Look up `T` in the calling thread's hot cache.
#[inline(always)]
fn cache_get<T: Injectable>(storage_ptr: usize, generation: u64) -> Option<Arc<T>> {
    HOT_CACHE
        .try_with(|cell| {
            // SAFETY: the thread-local is only accessed from this thread, and
            // no `&mut` to it is live: the mutable borrows in `cache_service`
            // and `cache_clear` end before any user code can run. `get` only
            // clones an Arc, so no user code runs during this borrow either.
            let cache = unsafe { &*cell.get() };
            cache.get::<T>(storage_ptr, generation)
        })
        .ok()
        .flatten()
}

/// Insert a resolved service into the hot cache, dropping any evicted entry
/// after the cache borrow ends.
///
/// Kept out of line so this cache-miss-only code stays out of `get()`.
#[inline(never)]
fn cache_service<T: Injectable>(storage_ptr: usize, generation: u64, service: Arc<T>) {
    let evicted = HOT_CACHE.try_with(|cell| {
        // SAFETY: single-threaded access (thread-local), and no other borrow
        // is live because user code never runs while one is held. `insert`
        // only moves values; the evicted entry is dropped below, after this
        // borrow ends.
        let cache = unsafe { &mut *cell.get() };
        cache.insert(storage_ptr, generation, service)
    });
    drop(evicted);
}

/// Clear the calling thread's hot cache, dropping the entries after the
/// cache borrow ends.
#[inline]
fn cache_clear() {
    let evicted = HOT_CACHE.try_with(|cell| {
        // SAFETY: as in `cache_service`; `clear` only moves the entries out.
        let cache = unsafe { &mut *cell.get() };
        cache.clear()
    });
    drop(evicted);
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

/// Non-owning handle to a [`Container`], for factories that need to resolve
/// from the container that owns them without creating an `Arc` cycle.
#[derive(Clone)]
pub(crate) struct WeakContainer {
    storage: std::sync::Weak<ServiceStorage>,
    parent_storage: Option<Arc<ServiceStorage>>,
    locked: Arc<AtomicBool>,
    depth: u32,
}

impl WeakContainer {
    /// Get the container back, or `None` if its storage has been dropped.
    #[inline]
    pub(crate) fn upgrade(&self) -> Option<Container> {
        Some(Container {
            storage: self.storage.upgrade()?,
            parent_storage: self.parent_storage.clone(),
            locked: Arc::clone(&self.locked),
            depth: self.depth,
        })
    }
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
    /// The factory may register, remove or clear services in this container
    /// (registration still panics once the container is locked), except when
    /// it is first resolved from inside a transient factory (see
    /// [`Container::transient`]). It must not resolve its own type, directly
    /// or through a dependency cycle: that blocks forever.
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
    /// The factory may resolve services from this container, but must not
    /// register, remove or clear services in it, directly or indirectly: it
    /// runs while the storage holds a read lock, so writing to the same
    /// container deadlocks. "Indirectly" includes a lazy service the factory
    /// resolves for the first time (its initializer runs inside the factory)
    /// and the `Drop` of any service the factory releases, including services
    /// of *any* container that a resolution inside the factory evicts from
    /// this thread's hot cache.
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
    ///
    /// # Panics
    ///
    /// Panics if `instance`'s concrete type does not match `type_id`, or if
    /// the container is locked.
    #[inline]
    pub fn register_by_id(&self, type_id: TypeId, instance: Arc<dyn Any + Send + Sync>) {
        // Resolution downcasts without re-checking the type, so the TypeId
        // must describe the instance exactly. Note `(*instance)`: calling
        // `type_id()` on the Arc itself would yield the Arc's TypeId.
        assert!(
            (*instance).type_id() == type_id,
            "register_by_id: instance's concrete type does not match type_id"
        );
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

        // Current storage generation - entries cached before a mutation won't match it
        let generation = self.storage.generation();

        // Phase 5+12: Check thread-local hot cache first (UnsafeCell, no RefCell overhead)
        // Note: Transients won't be in cache, so they'll fall through to get_and_cache
        if let Some(cached) = cache_get::<T>(storage_ptr, generation) {
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
        self.get_and_cache::<T>(storage_ptr, generation)
    }

    /// Internal: Resolve and cache a service
    ///
    /// Phase 15 optimization: Fast path for root containers (depth == 0) avoids
    /// function call overhead to resolve_from_parents when there are no parents.
    #[inline]
    fn get_and_cache<T: Injectable>(&self, storage_ptr: usize, generation: u64) -> Result<Arc<T>> {
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
                cache_service(storage_ptr, generation, Arc::clone(&service));
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
        self.resolve_from_parents::<T>(&type_id)
    }

    /// Resolve from parent chain (internal)
    ///
    /// Phase 9 optimization: Walks the full parent chain via ServiceStorage.parent.
    /// This allows services to be resolved from any ancestor scope.
    ///
    /// Phase 14 optimization: Marked as cold to improve branch prediction in the
    /// hot path - most resolutions hit the cache and don't need parent traversal.
    #[cold]
    fn resolve_from_parents<T: Injectable>(&self, type_id: &TypeId) -> Result<Arc<T>> {
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

                // Deliberately NOT hot-cached: an entry would be stamped with
                // the child's generation, which ancestor mutations never bump,
                // so a re-registration or clear() in the parent could serve a
                // stale value from this thread's cache indefinitely. Only
                // same-storage resolutions are cached (see `get_and_cache`);
                // this path is #[cold] and parent-chain walks stay correct.
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
    /// The cache is automatically invalidated whenever this container is
    /// mutated (services registered, removed, or cleared), so calling this
    /// manually is rarely necessary. It remains available for explicit
    /// control, e.g. to release the cached `Arc`s held by the current thread.
    ///
    /// Note: This clears the calling thread's entire hot cache, including
    /// entries cached for other containers.
    #[inline]
    pub fn clear_cache(&self) {
        cache_clear();
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

    /// Format a human-readable summary of every registration visible to this
    /// container, for logging when a resolution unexpectedly fails.
    ///
    /// The summary lists the scope depth, the number of services registered
    /// in each scope of the parent chain, and the [`TypeId`] of every
    /// registration. The container stores services by `TypeId` only (type
    /// names are not retained), so pair this output with the `type_name`
    /// carried by [`DiError::NotFound`] when diagnosing a failed resolve.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct MyService;
    ///
    /// let root = Container::new();
    /// root.singleton(MyService);
    ///
    /// let scope = root.scope();
    /// if scope.get::<String>().is_err() {
    ///     eprintln!("{}", scope.debug_registrations());
    /// }
    /// ```
    pub fn debug_registrations(&self) -> String {
        use std::fmt::Write;

        let mut out = format!("Container registrations (scope depth {}):\n", self.depth);
        let mut total = 0usize;
        let mut current = Some(&self.storage);
        let mut depth = self.depth;
        let mut is_local = true;

        while let Some(storage) = current {
            let type_ids = storage.type_ids();
            total += type_ids.len();
            let location = if is_local {
                "current scope"
            } else {
                "ancestor"
            };
            let _ = writeln!(
                out,
                "  {location} (depth {depth}): {} registered: {type_ids:?}",
                type_ids.len()
            );
            current = storage.parent();
            depth = depth.saturating_sub(1);
            is_local = false;
        }

        let _ = write!(out, "  total: {total} service(s) in scope chain");
        out
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
    /// Lazy services are initialized during the freeze (after the storage's
    /// locks are released), so the frozen storage is a snapshot: later
    /// `remove`/`clear` calls on the container do not affect it. It also
    /// keeps the container's services alive after the container is dropped.
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

    /// Create a non-owning handle to this container.
    #[inline]
    pub(crate) fn downgrade(&self) -> WeakContainer {
        WeakContainer {
            storage: Arc::downgrade(&self.storage),
            parent_storage: self.parent_storage.clone(),
            locked: Arc::clone(&self.locked),
            depth: self.depth,
        }
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

    /// Remove a service registration from this scope.
    ///
    /// Returns `true` if a service of type `T` was registered in this scope
    /// and has been removed, `false` otherwise. Does not affect parent scopes.
    /// Like [`clear`](Self::clear), removal is permitted on a locked
    /// container (locking prevents new registrations, not removal).
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
    /// assert!(container.remove::<MyService>());
    /// assert!(!container.contains::<MyService>());
    /// assert!(!container.remove::<MyService>()); // Already removed
    /// ```
    #[inline]
    pub fn remove<T: Injectable>(&self) -> bool {
        let removed = self.storage.remove(&TypeId::of::<T>());
        if removed {
            #[cfg(feature = "logging")]
            debug!(
                target: "dependency_injector",
                service = std::any::type_name::<T>(),
                depth = self.depth,
                "Service registration removed from this scope"
            );
        }
        removed
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
    ///
    /// See [`Container::transient`] for what the factory must not do.
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
    ///
    /// See [`Container::transient`] for what the factory must not do.
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
        // Clear storage for reuse (parent reference is preserved). The clear
        // stamps a fresh generation, invalidating hot cache entries for this
        // scope (including lingering clones of the container).
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

// Container is Send + Sync because every field is (Arc<ServiceStorage> over
// DashMap, Arc<AtomicBool>, u32). Checked at compile time rather than asserted
// with `unsafe impl`, so a future non-thread-safe field fails to build.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Container>();
};

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
    fn test_hot_cache_rejects_wrong_type_in_colliding_slot() {
        let storage_ptr = 0x1000;
        let generation = 7;
        let mut cache = HotCache::new();
        let _ = cache.insert(storage_ptr, generation, Arc::new(42u64));

        // Simulate a slot collision: put the u64 entry where u32 would look.
        let from = HotCache::slot_for::<u64>(storage_ptr);
        let to = HotCache::slot_for::<u32>(storage_ptr);
        let entry = cache.entries[from].take();
        cache.entries[to] = entry;

        assert!(cache.get::<u32>(storage_ptr, generation).is_none());
        cache.entries.swap(from, to);
        assert_eq!(*cache.get::<u64>(storage_ptr, generation).unwrap(), 42);
    }

    #[test]
    #[should_panic(expected = "does not match type_id")]
    fn test_register_by_id_rejects_mismatched_type() {
        let container = Container::new();
        container.register_by_id(TypeId::of::<String>(), Arc::new(7u8));
    }

    #[test]
    fn test_register_by_id_accepts_matching_type() {
        let container = Container::new();
        container.register_by_id(TypeId::of::<u8>(), Arc::new(7u8));
        assert_eq!(*container.get::<u8>().unwrap(), 7);
    }

    /// A container shared by the re-entrancy tests, resolved from `Drop`.
    fn reentry_target() -> &'static Container {
        static OTHER: std::sync::OnceLock<Container> = std::sync::OnceLock::new();
        OTHER.get_or_init(|| {
            let c = Container::new();
            c.singleton(5u16);
            c
        })
    }

    /// Resolves from another container (re-entering the hot cache) on drop.
    struct ResolvesOnDrop;
    impl Drop for ResolvesOnDrop {
        fn drop(&mut self) {
            assert_eq!(*reentry_target().get::<u16>().unwrap(), 5);
            let _ = RESOLVES_ON_DROP_DROPS.try_with(|n| n.set(n.get() + 1));
        }
    }

    thread_local! {
        /// Per-thread count of `ResolvesOnDrop` drops (tests run in parallel).
        static RESOLVES_ON_DROP_DROPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    fn resolves_on_drop_drops() -> usize {
        RESOLVES_ON_DROP_DROPS.with(std::cell::Cell::get)
    }

    #[test]
    fn test_hot_cache_slot_uses_low_pointer_bits() {
        // Containers allocated near each other differ in low address bits;
        // they must not all map a type to the same slot.
        let slots: std::collections::HashSet<usize> = (0..64usize)
            .map(|i| HotCache::slot_for::<u8>(0x1000 + i * 64))
            .collect();
        assert!(slots.len() > 1);
    }

    #[test]
    fn test_evicted_service_drop_can_resolve_services() {
        reentry_target();
        let container = Container::new();
        container.singleton(ResolvesOnDrop);
        let _ = container.get::<ResolvesOnDrop>().unwrap();
        drop(container);

        // The hot cache now holds the last Arc<ResolvesOnDrop>; clearing it
        // runs Drop, which must not observe a live mutable borrow.
        assert_eq!(resolves_on_drop_drops(), 0);
        Container::new().clear_cache();
        assert_eq!(resolves_on_drop_drops(), 1);
    }

    #[test]
    fn test_service_evicted_by_insert_can_resolve_services() {
        reentry_target();
        cache_clear();
        // Find a storage pointer where u8 lands in ResolvesOnDrop's slot.
        let storage_ptr = (1..=4096usize)
            .map(|i| i << 4)
            .find(|&ptr| HotCache::slot_for::<u8>(ptr) == HotCache::slot_for::<ResolvesOnDrop>(ptr))
            .expect("no storage pointer maps u8 into ResolvesOnDrop's slot");
        cache_service(storage_ptr, 1, Arc::new(ResolvesOnDrop));

        // Evicts ResolvesOnDrop, running its Drop from inside `cache_service`.
        assert_eq!(resolves_on_drop_drops(), 0);
        cache_service(storage_ptr, 1, Arc::new(1u8));
        assert_eq!(resolves_on_drop_drops(), 1);
        // (Not asserting u8 is cached: Drop's own cache insert may evict it.)
        assert!(cache_get::<ResolvesOnDrop>(storage_ptr, 1).is_none());
    }

    #[test]
    fn test_cached_service_drop_at_thread_exit_can_resolve_services() {
        use std::sync::atomic::AtomicUsize;

        static DROPS: AtomicUsize = AtomicUsize::new(0);

        struct ResolvesAtThreadExit;
        impl Drop for ResolvesAtThreadExit {
            fn drop(&mut self) {
                assert_eq!(*reentry_target().get::<u16>().unwrap(), 5);
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        reentry_target();
        std::thread::spawn(|| {
            let container = Container::new();
            container.singleton(ResolvesAtThreadExit);
            let _ = container.get::<ResolvesAtThreadExit>().unwrap();
            drop(container);
            // The thread-local cache's destructor drops the last Arc, and the
            // service resolves while the cache is being destroyed.
        })
        .join()
        .unwrap();
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_factories_run_outside_storage_locks() {
        static TARGET: std::sync::OnceLock<Container> = std::sync::OnceLock::new();

        #[derive(Debug)]
        struct Registers;

        let container = TARGET.get_or_init(Container::new);
        // `clear` write-locks every shard; it deadlocks if the lazy
        // initializer runs while `get` still holds the shard's read guard.
        container.lazy(|| {
            TARGET.get().unwrap().clear();
            Registers
        });
        container.get::<Registers>().unwrap();
        assert!(container.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "slow under Miri; covers dashmap lock fairness, not UB")]
    fn test_transient_reads_while_another_thread_writes() {
        use std::cell::Cell;
        use std::sync::atomic::AtomicBool;

        // Transient factories run under a shard read guard and may resolve
        // from the same container. That relies on DashMap admitting a reader
        // while a writer is merely *waiting*. Here the outer guard, the
        // writer and the nested reader all use the u64 shard, so a
        // writer-preferring lock deadlocks on every run.
        static TARGET: std::sync::OnceLock<Container> = std::sync::OnceLock::new();
        thread_local! {
            static NESTED: Cell<bool> = const { Cell::new(false) };
        }

        fn make() -> u64 {
            if NESTED.with(Cell::get) {
                return 7;
            }
            NESTED.with(|n| n.set(true));
            std::thread::yield_now();
            // Re-enters the same shard while the outer read guard is held.
            let value = *TARGET.get().unwrap().get::<u64>().unwrap();
            NESTED.with(|n| n.set(false));
            value
        }

        let container = TARGET.get_or_init(|| {
            let c = Container::new();
            c.transient(make);
            c
        });

        let stop = Arc::new(AtomicBool::new(false));
        let writer = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                // Re-registering u64 write-locks the shard the reader holds.
                while !stop.load(Ordering::Relaxed) {
                    TARGET.get().unwrap().transient(make);
                }
            })
        };
        for _ in 0..100_000 {
            assert_eq!(*container.get::<u64>().unwrap(), 7);
        }
        stop.store(true, Ordering::Relaxed);
        writer.join().unwrap();
    }

    #[test]
    fn test_clear_drops_services_outside_storage_locks() {
        static TARGET: std::sync::OnceLock<Container> = std::sync::OnceLock::new();

        struct InspectsOnDrop;
        impl Drop for InspectsOnDrop {
            fn drop(&mut self) {
                // `len` read-locks every shard; it deadlocks if `clear` still
                // holds a write lock while dropping services.
                let _ = TARGET.get().unwrap().len();
            }
        }

        let container = TARGET.get_or_init(Container::new);
        container.singleton(InspectsOnDrop);
        container.clear();
        assert!(container.is_empty());
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
    fn test_not_found_message_names_type_and_hints_at_diagnostics() {
        let container = Container::new();

        // Root fast path (get_and_cache)
        let message = container.get::<TestService>().err().unwrap().to_string();
        assert!(message.contains("TestService"));
        assert!(message.contains("different scope"));
        assert!(message.contains("Container::debug_registrations()"));

        // Parent-chain path (resolve_from_parents) produces the same message
        let scope = container.scope();
        let scoped_message = scope.get::<TestService>().err().unwrap().to_string();
        assert_eq!(message, scoped_message);
    }

    #[test]
    fn test_debug_registrations_reflects_count_and_depth() {
        let root = Container::new();
        root.singleton(TestService {
            value: "root".into(),
        });

        let child = root.scope();
        child.singleton(AnotherService {
            name: "child".into(),
        });

        let summary = child.debug_registrations();
        assert!(summary.contains("scope depth 1"));
        assert!(summary.contains("current scope (depth 1): 1 registered"));
        assert!(summary.contains("ancestor (depth 0): 1 registered"));
        assert!(summary.contains("total: 2 service(s) in scope chain"));

        // The root's summary only covers its own scope
        let root_summary = root.debug_registrations();
        assert!(root_summary.contains("scope depth 0"));
        assert!(root_summary.contains("current scope (depth 0): 1 registered"));
        assert!(!root_summary.contains("ancestor"));
        assert!(root_summary.contains("total: 1 service(s) in scope chain"));
    }

    #[test]
    fn test_debug_registrations_empty_container() {
        let container = Container::new();
        let summary = container.debug_registrations();
        assert!(summary.contains("current scope (depth 0): 0 registered"));
        assert!(summary.contains("total: 0 service(s) in scope chain"));
    }

    #[test]
    fn test_debug_registrations_lists_type_ids() {
        let container = Container::new();
        container.singleton(TestService {
            value: "listed".into(),
        });

        // The TypeId debug representation of the registered service appears
        let expected = format!("{:?}", TypeId::of::<TestService>());
        let summary = container.debug_registrations();
        assert!(summary.contains(&expected));
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

    #[test]
    fn test_cache_invalidated_on_clear() {
        let container = Container::new();
        container.singleton(TestService { value: "v1".into() });

        // Populate the thread-local hot cache
        assert_eq!(container.get::<TestService>().unwrap().value, "v1");

        // After clear(), the cached entry must not be served
        container.clear();
        assert!(container.get::<TestService>().is_err());
    }

    #[test]
    fn test_cache_invalidated_on_reregistration() {
        let container = Container::new();
        container.singleton(TestService { value: "v1".into() });

        // Populate the thread-local hot cache
        assert_eq!(container.get::<TestService>().unwrap().value, "v1");

        // Re-registering the same type overwrites the previous instance
        container.singleton(TestService { value: "v2".into() });
        assert_eq!(container.get::<TestService>().unwrap().value, "v2");
    }

    #[test]
    fn test_parent_mutation_visible_through_child_scope() {
        // Parent-resolved services are deliberately not hot-cached (see
        // resolve_from_parents), so mutations of an ancestor scope must be
        // visible through child scopes on the same thread.
        let root = Container::new();
        root.singleton(TestService { value: "v1".into() });

        let scope = root.scope();
        assert_eq!(scope.get::<TestService>().unwrap().value, "v1");

        // Re-register in the parent: the child must observe the new value
        root.singleton(TestService { value: "v2".into() });
        assert_eq!(scope.get::<TestService>().unwrap().value, "v2");

        // Clear the parent: the child must observe the removal
        root.clear();
        assert!(scope.get::<TestService>().is_err());
    }

    #[test]
    fn test_parent_singleton_identity_through_child() {
        // Parent-resolved services are not hot-cached; repeated resolution
        // through a child scope must still return the same Arc instance.
        let root = Container::new();
        root.singleton(TestService {
            value: "shared".into(),
        });

        let child = root.scope();
        let a = child.get::<TestService>().unwrap();
        let b = child.get::<TestService>().unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn test_remove_when_locked() {
        // Locking prevents new registrations; like clear(), remove is
        // still permitted (pinned behavior — see the remove() docs).
        let container = Container::new();
        container.singleton(TestService { value: "v".into() });
        container.lock();

        assert!(container.remove::<TestService>());
        assert!(!container.contains::<TestService>());
    }

    #[test]
    fn test_remove_in_child_does_not_affect_parent() {
        let root = Container::new();
        root.singleton(TestService {
            value: "root".into(),
        });

        let child = root.scope();
        // Removing a parent-only registration from the child is a no-op
        assert!(!child.remove::<TestService>());
        assert!(root.contains::<TestService>());

        // A child override can be removed without touching the parent
        child.singleton(TestService {
            value: "child".into(),
        });
        assert!(child.remove::<TestService>());
        assert!(root.contains::<TestService>());
        assert_eq!(root.get::<TestService>().unwrap().value, "root");
        // Child falls back to the parent's registration again
        assert_eq!(child.get::<TestService>().unwrap().value, "root");
    }

    #[test]
    fn test_pool_reuse_invalidates_hot_cache() {
        #[derive(Clone)]
        struct RequestId(u64);

        let root = Container::new();
        let pool = ScopePool::new(&root, 1);

        {
            let scope = pool.acquire();
            scope.singleton(RequestId(1));
            // Populate the thread-local hot cache for this storage
            assert_eq!(scope.get::<RequestId>().unwrap().0, 1);
        }

        // Same storage is reused; the release-path clear() must have
        // stamped a fresh generation so the cached RequestId(1) cannot hit.
        {
            let scope = pool.acquire();
            assert!(scope.get::<RequestId>().is_err());
            scope.singleton(RequestId(2));
            assert_eq!(scope.get::<RequestId>().unwrap().0, 2);
        }
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "parking_lot_core futex syscall is rejected by Miri's variadic arg check"
    )]
    fn test_cross_thread_invalidation_monotonic() {
        // A reader must never observe an older value after a newer one:
        // the Release stores in ServiceStorage's mutators paired with the
        // Acquire load in generation() make re-registrations publish to
        // readers' hot caches. (Single values are compared, so this test
        // is deterministic-safe: it can miss a regression but never
        // false-positives.)
        use std::sync::atomic::AtomicBool as StopFlag;

        #[derive(Clone)]
        struct Counter(u64);

        let container = Container::new();
        container.singleton(Counter(0));
        let done = Arc::new(StopFlag::new(false));

        let writer = {
            let c = container.clone();
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                for i in 1..=2_000u64 {
                    c.singleton(Counter(i));
                }
                done.store(true, Ordering::Release);
            })
        };

        let reader = {
            let c = container.clone();
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                let mut last = 0u64;
                while !done.load(Ordering::Acquire) {
                    let v = c.get::<Counter>().unwrap().0;
                    assert!(v >= last, "stale read: observed {v} after {last}");
                    last = v;
                }
            })
        };

        writer.join().unwrap();
        reader.join().unwrap();
    }

    #[test]
    fn test_mutation_does_not_affect_other_container_cache() {
        let a = Container::new();
        let b = Container::new();
        a.singleton(TestService { value: "a".into() });
        b.singleton(TestService { value: "b".into() });

        // Populate the hot cache for both containers
        assert_eq!(a.get::<TestService>().unwrap().value, "a");
        let b1 = b.get::<TestService>().unwrap();
        assert_eq!(b1.value, "b");

        // Mutating `a` must not disturb `b`'s cached resolution
        a.clear();
        assert!(a.get::<TestService>().is_err());
        let b2 = b.get::<TestService>().unwrap();
        assert_eq!(b2.value, "b");
        assert!(Arc::ptr_eq(&b1, &b2));
    }

    #[test]
    fn test_remove() {
        let container = Container::new();
        container.singleton(TestService {
            value: "test".into(),
        });
        assert!(container.contains::<TestService>());

        assert!(container.remove::<TestService>());
        assert!(!container.contains::<TestService>());
        assert!(container.get::<TestService>().is_err());
    }

    #[test]
    fn test_remove_unregistered() {
        let container = Container::new();
        assert!(!container.remove::<TestService>());
    }

    #[test]
    fn test_remove_invalidates_cache() {
        let container = Container::new();
        container.singleton(TestService {
            value: "cached".into(),
        });

        // Populate the thread-local hot cache
        assert_eq!(container.get::<TestService>().unwrap().value, "cached");

        // After remove(), the cached entry must not be served
        assert!(container.remove::<TestService>());
        assert!(container.get::<TestService>().is_err());
    }

    #[cfg(feature = "perfect-hash")]
    #[test]
    fn test_freeze() {
        let container = Container::new();
        container.singleton(TestService {
            value: "frozen".into(),
        });

        let frozen = container.freeze();

        // Freezing locks the container
        assert!(container.is_locked());

        // The frozen storage resolves the registered service
        let service = frozen.resolve(&TypeId::of::<TestService>()).unwrap();
        // SAFETY: We resolved by TypeId::of::<TestService>(), so the Arc
        // contains a TestService.
        let typed: Arc<TestService> = unsafe { downcast_arc_unchecked(service) };
        assert_eq!(typed.value, "frozen");
        assert_eq!(frozen.len(), 1);

        // The original container still resolves after freezing
        assert_eq!(container.get::<TestService>().unwrap().value, "frozen");
    }

    #[test]
    fn test_warm_cache() {
        let container = Container::new();
        container.singleton(TestService {
            value: "warm".into(),
        });

        // Pre-warm the cache, then resolve normally
        container.warm_cache::<TestService>();
        let service = container.get::<TestService>().unwrap();
        assert_eq!(service.value, "warm");
    }

    #[test]
    fn test_clear_cache() {
        let container = Container::new();
        container.singleton(TestService {
            value: "cached".into(),
        });

        // Populate the hot cache, then explicitly clear it
        let s1 = container.get::<TestService>().unwrap();
        container.clear_cache();

        // Resolution still works, served from storage again
        let s2 = container.get::<TestService>().unwrap();
        assert_eq!(s2.value, "cached");
        assert!(Arc::ptr_eq(&s1, &s2));
    }
}

//! High-performance storage for DI container
//!
//! Uses DashMap for lock-free concurrent access.
//! Optionally uses perfect hashing for locked containers (with `perfect-hash` feature).

#![allow(dead_code)]

use crate::factory::AnyFactory;
use ahash::RandomState;
use dashmap::DashMap;
use std::any::{Any, TypeId};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global source of unique cache generations.
///
/// Every storage starts with a fresh generation and is stamped with a new
/// one on every mutation (insert/clear/remove). Because generations are
/// globally unique, a hot cache entry stamped with an old generation can
/// never be mistaken for current — even if a freed storage's address is
/// reused by a new storage (ABA).
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Get a fresh, globally unique generation value.
#[inline]
fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
}

#[cfg(feature = "perfect-hash")]
use std::hash::{Hash, Hasher};

// =============================================================================
// Unchecked Downcast (Phase 8 optimization)
// =============================================================================

/// Downcast an `Arc<dyn Any + Send + Sync>` to `Arc<T>` without runtime type checking.
///
/// This uses the same pattern as std's [`Arc::downcast`], but skips the `is::<T>()` check
/// because callers guarantee type correctness via `TypeId` lookup.
///
/// # Safety
///
/// The caller MUST guarantee that `arc` was originally created from an `Arc<T>`.
/// This is typically ensured by looking up via `TypeId::of::<T>()`.
///
/// # Implementation Notes
///
/// When casting `*const dyn Any` (fat pointer) to `*const T` (thin pointer), Rust
/// extracts just the data pointer portion, discarding the vtable metadata. This is
/// well-defined behavior and exactly how std's `Arc::downcast` works:
///
/// ```ignore
/// // From std::sync::Arc::downcast:
/// unsafe {
///     let ptr = Arc::into_raw(self);
///     Ok(Arc::from_raw(ptr as *const T))
/// }
/// ```
///
/// # Crate Invariants
///
/// In this crate, safety is guaranteed because:
/// - Factories are keyed by `TypeId::of::<T>()` at registration time
/// - Resolution looks up by the same `TypeId::of::<T>()`
/// - The factory stores the exact type that was registered
/// - TypeId comparison is the same check that `Any::is::<T>()` performs
#[inline]
pub(crate) unsafe fn downcast_arc_unchecked<T: Send + Sync + 'static>(
    arc: Arc<dyn Any + Send + Sync>,
) -> Arc<T> {
    // SAFETY: Caller guarantees arc contains type T (verified via TypeId lookup).
    // This is the same pattern used by std::sync::Arc::downcast.
    let ptr = Arc::into_raw(arc);
    // Fat-to-thin pointer cast extracts the data pointer, discarding vtable metadata.
    unsafe { Arc::from_raw(ptr as *const T) }
}

/// Thread-safe storage for service factories
///
/// Uses `DashMap` with `ahash` for maximum concurrent performance.
/// Supports hierarchical parent chain for deep scope resolution.
pub struct ServiceStorage {
    /// Map from TypeId to factory
    factories: DashMap<TypeId, AnyFactory, RandomState>,
    /// Optional parent storage for hierarchical resolution
    parent: Option<Arc<ServiceStorage>>,
    /// Cache generation — stamped with a globally unique value on every
    /// mutation so thread-local hot-cache entries from before the mutation
    /// can be detected as stale. Stamping happens inside `insert`/`clear`/
    /// `remove`, the only mutators, so no caller can forget to invalidate.
    generation: AtomicU64,
}

impl ServiceStorage {
    /// Create new empty storage with optimized shard count.
    ///
    /// Uses 8 shards as a balance between:
    /// - Creation overhead (fewer shards = faster creation)
    /// - Concurrent read performance (more shards = less contention)
    ///
    /// Default DashMap uses num_cpus * 4 shards which is overkill for
    /// typical DI containers with <50 services.
    #[inline]
    pub fn new() -> Self {
        Self {
            factories: DashMap::with_capacity_and_hasher_and_shard_amount(
                0,
                RandomState::new(),
                8, // 8 shards balances creation speed vs concurrency
            ),
            parent: None,
            generation: AtomicU64::new(next_generation()),
        }
    }

    /// Create with pre-allocated capacity and optimized shards.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        // Scale shards based on expected capacity and concurrency needs
        let shard_amount = if capacity <= 16 {
            8
        } else if capacity <= 64 {
            16
        } else {
            32
        };
        Self {
            factories: DashMap::with_capacity_and_hasher_and_shard_amount(
                capacity,
                RandomState::new(),
                shard_amount,
            ),
            parent: None,
            generation: AtomicU64::new(next_generation()),
        }
    }

    /// Create a child storage with a parent reference for deep hierarchy resolution.
    ///
    /// Uses only 4 shards since child scopes typically have very few services
    /// (usually just request-specific data like RequestId). This reduces creation
    /// overhead while still supporting concurrent access.
    #[inline]
    pub fn with_parent(parent: Arc<ServiceStorage>) -> Self {
        Self {
            factories: DashMap::with_capacity_and_hasher_and_shard_amount(
                0,
                RandomState::new(),
                4, // 4 shards for child scopes (typically <5 services)
            ),
            parent: Some(parent),
            generation: AtomicU64::new(next_generation()),
        }
    }

    /// Insert a factory
    #[inline]
    pub(crate) fn insert(&self, type_id: TypeId, factory: AnyFactory) {
        self.factories.insert(type_id, factory);
        // Release pairs with the Acquire in `generation()`: a reader that
        // observes the new generation also observes the map mutation, keeping
        // the invalidation protocol self-contained (no reliance on DashMap's
        // internal shard locks for ordering).
        self.generation.store(next_generation(), Ordering::Release);
    }

    /// Check if type exists
    #[inline]
    pub fn contains(&self, type_id: &TypeId) -> bool {
        self.factories.contains_key(type_id)
    }

    /// Resolve a service by TypeId
    #[inline]
    pub fn resolve(&self, type_id: &TypeId) -> Option<Arc<dyn Any + Send + Sync>> {
        self.factories.get(type_id).map(|f| f.resolve())
    }

    /// Try to resolve and downcast to T
    ///
    /// Uses unchecked downcast since we know the type from the TypeId lookup.
    #[inline]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.resolve(&TypeId::of::<T>()).map(|any| {
            // SAFETY: We looked up by TypeId::of::<T>(), so the factory
            // was registered with the same TypeId and stores type T.
            unsafe { downcast_arc_unchecked(any) }
        })
    }

    /// Resolve and return both the service and whether it's transient.
    ///
    /// This avoids a second DashMap lookup when checking if the service should be cached.
    /// Returns `Some((service, is_transient))` if found, `None` if not found.
    #[inline]
    pub fn get_with_transient_flag<T: Send + Sync + 'static>(&self) -> Option<(Arc<T>, bool)> {
        let type_id = TypeId::of::<T>();
        self.factories.get(&type_id).map(|factory| {
            let is_transient = factory.is_transient();
            let service = factory.resolve();
            // SAFETY: We looked up by TypeId::of::<T>(), so the factory stores type T.
            let typed = unsafe { downcast_arc_unchecked(service) };
            (typed, is_transient)
        })
    }

    /// Resolve a service by walking the full parent chain.
    ///
    /// Returns the service from the nearest scope that has it registered.
    #[inline]
    pub fn resolve_from_chain(&self, type_id: &TypeId) -> Option<Arc<dyn Any + Send + Sync>> {
        // Check current scope first
        if let Some(service) = self.resolve(type_id) {
            return Some(service);
        }

        // Walk parent chain
        let mut current = self.parent.as_ref();
        while let Some(storage) = current {
            if let Some(service) = storage.resolve(type_id) {
                return Some(service);
            }
            current = storage.parent.as_ref();
        }

        None
    }

    /// Check if a service exists in this storage or any parent.
    #[inline]
    pub fn contains_in_chain(&self, type_id: &TypeId) -> bool {
        // Check current scope first
        if self.contains(type_id) {
            return true;
        }

        // Walk parent chain
        let mut current = self.parent.as_ref();
        while let Some(storage) = current {
            if storage.contains(type_id) {
                return true;
            }
            current = storage.parent.as_ref();
        }

        false
    }

    /// Get reference to parent storage (if any)
    #[inline]
    pub fn parent(&self) -> Option<&Arc<ServiceStorage>> {
        self.parent.as_ref()
    }

    /// Create a child storage from this storage.
    ///
    /// This is more efficient than `with_parent` as it takes self by Arc reference.
    #[inline]
    pub fn child(self: &Arc<Self>) -> Self {
        Self {
            factories: DashMap::with_capacity_and_hasher_and_shard_amount(0, RandomState::new(), 8),
            parent: Some(Arc::clone(self)),
            generation: AtomicU64::new(next_generation()),
        }
    }

    /// Get number of registered services
    #[inline]
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Clear all services (preserves parent reference)
    #[inline]
    pub fn clear(&self) {
        self.factories.clear();
        // Release: see `insert`.
        self.generation.store(next_generation(), Ordering::Release);
    }

    /// Check if this storage has a parent
    #[inline]
    pub fn has_parent(&self) -> bool {
        self.parent.is_some()
    }

    /// Remove a service
    #[inline]
    pub fn remove(&self, type_id: &TypeId) -> bool {
        let removed = self.factories.remove(type_id).is_some();
        if removed {
            // Release: see `insert`.
            self.generation.store(next_generation(), Ordering::Release);
        }
        removed
    }

    /// Get all registered type IDs
    pub fn type_ids(&self) -> Vec<TypeId> {
        self.factories.iter().map(|r| *r.key()).collect()
    }

    /// Check if a service is transient
    #[inline]
    pub fn is_transient(&self, type_id: &TypeId) -> bool {
        self.factories
            .get(type_id)
            .map(|f| f.is_transient())
            .unwrap_or(false)
    }

    /// Current cache generation of this storage.
    ///
    /// Changes on every mutation; hot-cache entries stamped with an older
    /// generation are stale.
    #[inline]
    pub(crate) fn generation(&self) -> u64 {
        // Acquire pairs with the Release stores in insert/clear/remove.
        // NOTE: `Container::get` must load this BEFORE reading the factories
        // map so that an entry cached under this generation can never contain
        // data newer than the generation claims.
        self.generation.load(Ordering::Acquire)
    }
}

impl Default for ServiceStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ServiceStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceStorage")
            .field("count", &self.len())
            .finish()
    }
}

// =============================================================================
// Perfect Hashing for Frozen Storage (Phase 10 optimization)
// =============================================================================

/// Wrapper for TypeId that implements Hash trait for boomphf
#[cfg(feature = "perfect-hash")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct HashableTypeId(TypeId);

#[cfg(feature = "perfect-hash")]
impl Hash for HashableTypeId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // TypeId is already a hash, just pass it through
        // Use the debug representation to get the internal value
        let type_id_bits: u64 = unsafe { std::mem::transmute_copy(&self.0) };
        type_id_bits.hash(state);
    }
}

/// Frozen storage with perfect hashing for O(1) lookups.
///
/// Created from a `ServiceStorage` after the container is locked.
/// Uses minimal perfect hashing (MPHF) for ~5ns faster resolution.
#[cfg(feature = "perfect-hash")]
pub struct FrozenStorage {
    /// The perfect hash function
    mphf: boomphf::Mphf<HashableTypeId>,
    /// Factories indexed by perfect hash
    factories: Vec<AnyFactory>,
    /// TypeIds for verification (optional, can be removed for speed)
    type_ids: Vec<TypeId>,
    /// Parent storage for hierarchical resolution
    parent: Option<Arc<FrozenStorage>>,
}

#[cfg(feature = "perfect-hash")]
impl FrozenStorage {
    /// Create a frozen storage from a ServiceStorage.
    ///
    /// This computes a minimal perfect hash function for all registered TypeIds,
    /// enabling O(1) lookups without hash collisions.
    pub fn from_storage(storage: &ServiceStorage) -> Self {
        // Collect all entries as owned values
        let entries: Vec<(TypeId, AnyFactory)> = storage
            .factories
            .iter()
            .map(|r| (*r.key(), r.value().clone()))
            .collect();

        let n = entries.len();
        if n == 0 {
            return Self {
                mphf: boomphf::Mphf::new(1.7, &[]),
                factories: Vec::new(),
                type_ids: Vec::new(),
                parent: storage
                    .parent
                    .as_ref()
                    .map(|p| Arc::new(Self::from_storage(p))),
            };
        }

        // Create hashable type IDs for MPHF
        let hashable_ids: Vec<HashableTypeId> =
            entries.iter().map(|(id, _)| HashableTypeId(*id)).collect();

        // Create MPHF with gamma=1.7 (good balance of speed vs memory)
        let mphf = boomphf::Mphf::new(1.7, &hashable_ids);

        // Create factory and type_id arrays indexed by perfect hash
        let mut factories: Vec<Option<AnyFactory>> = (0..n).map(|_| None).collect();
        let mut indexed_type_ids: Vec<Option<TypeId>> = (0..n).map(|_| None).collect();

        for (type_id, factory) in entries {
            let idx = mphf.hash(&HashableTypeId(type_id)) as usize;
            factories[idx] = Some(factory);
            indexed_type_ids[idx] = Some(type_id);
        }

        // Unwrap all Options (all slots should be filled)
        let factories: Vec<AnyFactory> = factories.into_iter().flatten().collect();
        let type_ids: Vec<TypeId> = indexed_type_ids.into_iter().flatten().collect();

        // Freeze parent if it exists
        let parent = storage
            .parent
            .as_ref()
            .map(|p| Arc::new(Self::from_storage(p)));

        Self {
            mphf,
            factories,
            type_ids,
            parent,
        }
    }

    /// Resolve a service by TypeId using perfect hashing.
    ///
    /// This is O(1) with no hash collisions.
    #[inline]
    pub fn resolve(&self, type_id: &TypeId) -> Option<Arc<dyn Any + Send + Sync>> {
        let hashable = HashableTypeId(*type_id);

        // Try to get hash - returns None if type_id wasn't in original set
        let idx = self.mphf.try_hash(&hashable)? as usize;

        // Bounds check (should always pass if MPHF is correct)
        if idx >= self.factories.len() {
            return None;
        }

        // Verify TypeId matches (MPHF can give false positives for unknown keys)
        if self.type_ids[idx] != *type_id {
            return None;
        }

        Some(self.factories[idx].resolve())
    }

    /// Check if a type exists in this frozen storage.
    #[inline]
    pub fn contains(&self, type_id: &TypeId) -> bool {
        let hashable = HashableTypeId(*type_id);

        if let Some(idx) = self.mphf.try_hash(&hashable) {
            let idx = idx as usize;
            idx < self.type_ids.len() && self.type_ids[idx] == *type_id
        } else {
            false
        }
    }

    /// Resolve from the full parent chain.
    #[inline]
    pub fn resolve_from_chain(&self, type_id: &TypeId) -> Option<Arc<dyn Any + Send + Sync>> {
        if let Some(service) = self.resolve(type_id) {
            return Some(service);
        }

        let mut current = self.parent.as_ref();
        while let Some(storage) = current {
            if let Some(service) = storage.resolve(type_id) {
                return Some(service);
            }
            current = storage.parent.as_ref();
        }

        None
    }

    /// Check if type exists in chain.
    #[inline]
    pub fn contains_in_chain(&self, type_id: &TypeId) -> bool {
        if self.contains(type_id) {
            return true;
        }

        let mut current = self.parent.as_ref();
        while let Some(storage) = current {
            if storage.contains(type_id) {
                return true;
            }
            current = storage.parent.as_ref();
        }

        false
    }

    /// Get the number of services.
    #[inline]
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Check if a service is transient.
    #[inline]
    pub fn is_transient(&self, type_id: &TypeId) -> bool {
        let hashable = HashableTypeId(*type_id);

        if let Some(idx) = self.mphf.try_hash(&hashable) {
            let idx = idx as usize;
            if idx < self.factories.len() && self.type_ids[idx] == *type_id {
                return self.factories[idx].is_transient();
            }
        }
        false
    }
}

#[cfg(feature = "perfect-hash")]
impl std::fmt::Debug for FrozenStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrozenStorage")
            .field("count", &self.len())
            .field("has_parent", &self.parent.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestService {
        value: i32,
    }

    #[cfg(feature = "perfect-hash")]
    #[test]
    fn test_frozen_storage() {
        let storage = ServiceStorage::new();
        storage.insert(
            TypeId::of::<TestService>(),
            AnyFactory::singleton(TestService { value: 42 }),
        );
        storage.insert(TypeId::of::<i32>(), AnyFactory::singleton(123i32));
        storage.insert(
            TypeId::of::<String>(),
            AnyFactory::singleton("hello".to_string()),
        );

        let frozen = FrozenStorage::from_storage(&storage);

        // Test contains
        assert!(frozen.contains(&TypeId::of::<TestService>()));
        assert!(frozen.contains(&TypeId::of::<i32>()));
        assert!(frozen.contains(&TypeId::of::<String>()));
        assert!(!frozen.contains(&TypeId::of::<bool>())); // Not registered

        // Test resolve
        let service = frozen.resolve(&TypeId::of::<TestService>()).unwrap();
        let typed: Arc<TestService> = unsafe { downcast_arc_unchecked(service) };
        assert_eq!(typed.value, 42);

        // Test len
        assert_eq!(frozen.len(), 3);
    }

    #[cfg(feature = "perfect-hash")]
    #[test]
    fn test_frozen_storage_empty() {
        let storage = ServiceStorage::new();
        let frozen = FrozenStorage::from_storage(&storage);

        assert!(frozen.is_empty());
        assert_eq!(frozen.len(), 0);
        assert!(!frozen.contains(&TypeId::of::<TestService>()));
    }

    #[test]
    fn test_storage_insert_and_get() {
        let storage = ServiceStorage::new();
        let type_id = TypeId::of::<TestService>();

        // Phase 2: Use new enum-based AnyFactory API
        storage.insert(type_id, AnyFactory::singleton(TestService { value: 42 }));

        let service = storage.get::<TestService>().unwrap();
        assert_eq!(service.value, 42);
    }

    #[test]
    fn test_storage_contains() {
        let storage = ServiceStorage::new();
        let type_id = TypeId::of::<TestService>();

        assert!(!storage.contains(&type_id));

        storage.insert(type_id, AnyFactory::singleton(TestService { value: 0 }));

        assert!(storage.contains(&type_id));
    }

    #[test]
    fn test_storage_remove() {
        let storage = ServiceStorage::new();
        let type_id = TypeId::of::<TestService>();

        storage.insert(type_id, AnyFactory::singleton(TestService { value: 0 }));
        assert!(storage.contains(&type_id));

        storage.remove(&type_id);
        assert!(!storage.contains(&type_id));
    }

    /// Verify that unchecked downcast correctly handles fat-to-thin pointer conversion.
    /// This test ensures the optimization is sound by checking pointer equality.
    #[test]
    fn test_unchecked_downcast_soundness() {
        use std::any::Any;

        // Create an Arc<T> and record its data pointer
        let original: Arc<TestService> = Arc::new(TestService { value: 42 });
        let original_ptr = Arc::as_ptr(&original);

        // Coerce to Arc<dyn Any + Send + Sync>
        let any_arc: Arc<dyn Any + Send + Sync> = original;

        // Downcast back using our unchecked function
        // SAFETY: We know the type is TestService
        let recovered: Arc<TestService> = unsafe { super::downcast_arc_unchecked(any_arc) };

        // Verify pointer equality (proves fat→thin cast is correct)
        assert_eq!(original_ptr, Arc::as_ptr(&recovered));

        // Verify value is intact
        assert_eq!(recovered.value, 42);
    }
}

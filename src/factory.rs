//! Factory types for creating service instances
//!
//! Factories encapsulate how services are created and their lifecycle.
//!
//! ## Phase 2 Optimizations
//!
//! This module uses an enum-based `AnyFactory` instead of trait objects to:
//! - Eliminate vtable indirection on every resolve (~2-3ns savings)
//! - Store type-erased `Arc<dyn Any>` directly to avoid clone+cast overhead
//! - Enable better inlining opportunities

use crate::Injectable;
use once_cell::sync::OnceCell;
use std::any::Any;
use std::sync::Arc;

#[cfg(feature = "logging")]
use tracing::{debug, trace};

/// A factory that creates service instances (trait for external extensibility)
pub trait Factory: Send + Sync {
    /// Resolve the service, creating it if necessary
    fn resolve(&self) -> Arc<dyn Any + Send + Sync>;

    /// Check if this factory produces a new instance each time
    fn is_transient(&self) -> bool {
        false
    }
}

// =============================================================================
// Singleton Factory
// =============================================================================

/// Singleton factory - stores a single pre-created instance
///
/// Optimization: Stores type-erased `Arc<dyn Any>` directly to avoid
/// clone+cast on every resolution.
pub struct SingletonFactory {
    /// Pre-erased instance - avoids cast overhead on resolve
    pub(crate) instance: Arc<dyn Any + Send + Sync>,
}

impl SingletonFactory {
    /// Create from an existing instance
    #[inline]
    pub fn new<T: Injectable>(instance: T) -> Self {
        Self {
            instance: Arc::new(instance) as Arc<dyn Any + Send + Sync>,
        }
    }

    /// Create from an Arc
    #[inline]
    pub fn from_arc<T: Injectable>(instance: Arc<T>) -> Self {
        Self {
            instance: instance as Arc<dyn Any + Send + Sync>,
        }
    }

    /// Resolve the instance (just clones the Arc, no cast needed)
    #[inline]
    pub fn resolve(&self) -> Arc<dyn Any + Send + Sync> {
        Arc::clone(&self.instance)
    }
}

impl Factory for SingletonFactory {
    #[inline]
    fn resolve(&self) -> Arc<dyn Any + Send + Sync> {
        self.resolve()
    }
}

// =============================================================================
// Lazy Factory
// =============================================================================

/// Type-erased factory function
type LazyInitFn = dyn Fn() -> Arc<dyn Any + Send + Sync> + Send + Sync;

/// Lazy singleton factory - creates instance on first access
///
/// Optimization: Stores type-erased factory and instance to avoid
/// generic monomorphization overhead in hot paths.
pub struct LazyFactory {
    /// Shared so clones (handles taken out of storage before resolving, or
    /// frozen copies) initialize and observe the same instance.
    state: Arc<LazyState<LazyInitFn>>,
}

/// Generic over the init closure so it lives in the same allocation as the
/// cell; `Arc<LazyState<F>>` unsizes to `Arc<LazyState<LazyInitFn>>`.
struct LazyState<F: ?Sized> {
    /// Cached instance (type-erased)
    instance: OnceCell<Arc<dyn Any + Send + Sync>>,
    /// Type name for logging
    #[cfg(feature = "logging")]
    type_name: &'static str,
    /// Type-erased factory function (last: it is the unsized field)
    init: F,
}

impl LazyFactory {
    /// Create a new lazy factory
    #[inline]
    pub fn new<T: Injectable, F>(factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            state: Arc::new(LazyState {
                instance: OnceCell::new(),
                #[cfg(feature = "logging")]
                type_name: std::any::type_name::<T>(),
                init: move || Arc::new(factory()) as Arc<dyn Any + Send + Sync>,
            }),
        }
    }

    /// Get the instance if it has already been created
    #[inline]
    pub(crate) fn get(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        self.state.instance.get().map(Arc::clone)
    }

    /// Get the instance, creating it if necessary
    #[inline]
    pub fn resolve(&self) -> Arc<dyn Any + Send + Sync> {
        #[cfg(feature = "logging")]
        let was_empty = self.state.instance.get().is_none();

        let result = Arc::clone(self.state.instance.get_or_init(|| {
            #[cfg(feature = "logging")]
            debug!(
                target: "dependency_injector",
                service = self.state.type_name,
                "Lazy singleton initializing on first access"
            );

            (self.state.init)()
        }));

        #[cfg(feature = "logging")]
        if !was_empty {
            trace!(
                target: "dependency_injector",
                service = self.state.type_name,
                "Lazy singleton already initialized, returning cached instance"
            );
        }

        result
    }
}

impl Factory for LazyFactory {
    #[inline]
    fn resolve(&self) -> Arc<dyn Any + Send + Sync> {
        self.resolve()
    }
}

// =============================================================================
// Transient Factory
// =============================================================================

/// Type-erased transient factory function
type TransientFn = Arc<dyn Fn() -> Arc<dyn Any + Send + Sync> + Send + Sync>;

/// Transient factory - creates new instance every time
///
/// Optimization: Stores type-erased factory to avoid generic overhead.
pub struct TransientFactory {
    /// Type-erased factory function
    factory: TransientFn,
    /// Type name for logging
    #[cfg(feature = "logging")]
    type_name: &'static str,
}

impl TransientFactory {
    /// Create a new transient factory
    #[inline]
    pub fn new<T: Injectable, F>(factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            factory: Arc::new(move || Arc::new(factory()) as Arc<dyn Any + Send + Sync>),
            #[cfg(feature = "logging")]
            type_name: std::any::type_name::<T>(),
        }
    }

    /// Create a new instance
    #[inline]
    pub fn create(&self) -> Arc<dyn Any + Send + Sync> {
        #[cfg(feature = "logging")]
        trace!(
            target: "dependency_injector",
            service = self.type_name,
            "Creating new transient instance"
        );

        (self.factory)()
    }
}

impl Factory for TransientFactory {
    #[inline]
    fn resolve(&self) -> Arc<dyn Any + Send + Sync> {
        self.create()
    }

    #[inline]
    fn is_transient(&self) -> bool {
        true
    }
}

// =============================================================================
// AnyFactory - Enum-based type erasure (Phase 2 optimization)
// =============================================================================

/// Type-erased factory wrapper for storage
///
/// ## Optimization: Enum vs Trait Object
///
/// Using an enum instead of `Box<dyn Factory>` eliminates:
/// - vtable pointer lookup on every resolve (~2-3ns)
/// - indirect function call overhead
/// - better branch prediction (enum discriminant vs vtable)
///
/// The match on 3 variants is cheaper than a vtable lookup because:
/// - Enum discriminant is a single byte comparison
/// - All code paths are visible to the optimizer
/// - Better cache locality (no pointer chasing)
pub(crate) enum AnyFactory {
    /// Eager singleton - instance already created
    Singleton(SingletonFactory),
    /// Lazy singleton - created on first access
    Lazy(LazyFactory),
    /// Transient - new instance each time
    Transient(TransientFactory),
    /// Holds nothing; dropping it runs no user code. Only exists transiently
    /// inside `ServiceStorage::clear` and is never stored or resolved.
    Vacant,
}

/// Panic for the `Vacant` invariant (kept out of line, off the hot paths).
#[cold]
#[inline(never)]
fn vacant_factory_used() -> ! {
    unreachable!("vacant factory is never stored or resolved")
}

impl Clone for AnyFactory {
    /// Clone the factory. Cheap and never runs user code: lazy clones share
    /// the original's state (and therefore its instance), and transient
    /// clones share the factory closure.
    fn clone(&self) -> Self {
        match self {
            AnyFactory::Singleton(f) => AnyFactory::Singleton(SingletonFactory {
                instance: Arc::clone(&f.instance),
            }),
            AnyFactory::Lazy(f) => AnyFactory::Lazy(LazyFactory {
                state: Arc::clone(&f.state),
            }),
            AnyFactory::Transient(f) => AnyFactory::Transient(TransientFactory {
                factory: Arc::clone(&f.factory),
                #[cfg(feature = "logging")]
                type_name: f.type_name,
            }),
            AnyFactory::Vacant => AnyFactory::Vacant,
        }
    }
}

/// Outcome of [`AnyFactory::resolve_ready`].
pub(crate) enum Resolution {
    /// The instance already exists; no user code ran.
    Ready(Arc<dyn Any + Send + Sync>),
    /// A lazy service is not initialized yet; initializing it runs user code.
    /// Holds a cheap clone of the factory (sharing its state) so the caller
    /// can release any lock before calling [`AnyFactory::resolve`].
    Deferred(AnyFactory),
}

impl AnyFactory {
    /// Create a singleton factory
    #[inline]
    pub fn singleton<T: Injectable>(instance: T) -> Self {
        AnyFactory::Singleton(SingletonFactory::new(instance))
    }

    /// Create a lazy factory
    #[inline]
    pub fn lazy<T: Injectable, F>(factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        AnyFactory::Lazy(LazyFactory::new(factory))
    }

    /// Create a transient factory
    #[inline]
    pub fn transient<T: Injectable, F>(factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        AnyFactory::Transient(TransientFactory::new(factory))
    }

    /// Resolve the service
    #[inline]
    pub fn resolve(&self) -> Arc<dyn Any + Send + Sync> {
        match self {
            AnyFactory::Singleton(f) => f.resolve(),
            AnyFactory::Lazy(f) => f.resolve(),
            AnyFactory::Transient(f) => f.create(),
            AnyFactory::Vacant => vacant_factory_used(),
        }
    }

    /// Initialize a lazy factory and return it as a singleton holding the
    /// (shared) instance; other factories are returned unchanged.
    pub(crate) fn into_initialized(self) -> Self {
        match self {
            AnyFactory::Lazy(f) => AnyFactory::Singleton(SingletonFactory {
                instance: f.resolve(),
            }),
            other => other,
        }
    }

    /// Resolve, deferring lazy initialization (see [`Resolution`]).
    ///
    /// Transient factories run here, in place: deferring them would cost an
    /// extra refcount round-trip on every resolution. Storage holds only a
    /// shard *read* guard while they run. dashmap 6.1-6.2's `RawRwLock`
    /// admits readers unless a writer *holds* it (a waiting writer does not
    /// block them; this is an implementation detail, not dashmap API), so
    /// a transient factory may resolve services but must not register,
    /// remove or clear them in the same container, directly or indirectly
    /// (see `Container::transient`). The reader-admission behaviour is
    /// covered by `test_transient_reads_while_another_thread_writes`.
    #[inline]
    pub(crate) fn resolve_ready(&self) -> Resolution {
        match self {
            AnyFactory::Singleton(f) => Resolution::Ready(f.resolve()),
            AnyFactory::Lazy(f) => match f.get() {
                Some(instance) => Resolution::Ready(instance),
                None => Resolution::Deferred(self.clone()),
            },
            AnyFactory::Transient(f) => Resolution::Ready(f.create()),
            AnyFactory::Vacant => vacant_factory_used(),
        }
    }

    /// Check if transient
    #[inline]
    pub fn is_transient(&self) -> bool {
        matches!(self, AnyFactory::Transient(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Clone)]
    struct TestService {
        id: u32,
    }

    #[test]
    fn test_singleton_factory() {
        let factory = AnyFactory::singleton(TestService { id: 42 });

        let a = factory.resolve();
        let b = factory.resolve();

        let a = a.downcast::<TestService>().unwrap();
        let b = b.downcast::<TestService>().unwrap();

        assert_eq!(a.id, 42);
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn test_lazy_factory() {
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        let factory = AnyFactory::lazy(|| TestService {
            id: COUNTER.fetch_add(1, Ordering::SeqCst),
        });

        assert_eq!(COUNTER.load(Ordering::SeqCst), 0);

        let a = factory.resolve().downcast::<TestService>().unwrap();
        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
        assert_eq!(a.id, 0);

        let b = factory.resolve().downcast::<TestService>().unwrap();
        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn test_transient_factory() {
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        let factory = AnyFactory::transient(|| TestService {
            id: COUNTER.fetch_add(1, Ordering::SeqCst),
        });

        let a = factory.resolve().downcast::<TestService>().unwrap();
        let b = factory.resolve().downcast::<TestService>().unwrap();

        assert_eq!(a.id, 0);
        assert_eq!(b.id, 1);
        assert!(!Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn test_is_transient() {
        let singleton = AnyFactory::singleton(TestService { id: 1 });
        let lazy = AnyFactory::lazy(|| TestService { id: 2 });
        let transient = AnyFactory::transient(|| TestService { id: 3 });

        assert!(!singleton.is_transient());
        assert!(!lazy.is_transient());
        assert!(transient.is_transient());
    }
}

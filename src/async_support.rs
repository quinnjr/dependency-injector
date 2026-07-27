//! Async support for the dependency injection container.
//!
//! Available when the `async` feature is enabled, this module adds
//! *async-initialized singletons* on top of the synchronous API:
//!
//! - [`Container::lazy_async`] registers a singleton whose value is produced
//!   by an async factory on first resolution.
//! - [`Container::get_async`] resolves a service, awaiting the async factory
//!   (instead of blocking a thread) if the value is still initializing. For
//!   types registered through the synchronous API it behaves exactly like
//!   [`Container::get`].
//! - [`Container::try_get_async`] is the [`Container::try_get`] counterpart.
//!
//! # Scope of async support
//!
//! Only *singleton initialization* is async: the factory passed to
//! [`Container::lazy_async`] runs to completion exactly once, and every
//! subsequent resolve returns the cached `Arc<T>`. Transients, sync lazy singletons, factories, and the
//! rest of the container API remain fully synchronous — there is no async
//! transient or async scoped lifetime.
//!
//! # How it works
//!
//! [`Container::lazy_async`] does not store `T` directly. It registers an
//! [`AsyncLazy<T>`] wrapper (a `tokio::sync::OnceCell` plus the boxed async
//! factory) as a regular singleton via [`Container::singleton`], so async
//! registrations participate in scoping and parent-chain resolution like any
//! other service. [`Container::get_async`] first looks for the wrapper and
//! awaits its cell; if no wrapper is registered it falls back to the
//! synchronous [`Container::get`].
//!
//! Because the registration is keyed as [`AsyncLazy<T>`] rather than `T`, use
//! `contains::<AsyncLazy<T>>()` / `remove::<AsyncLazy<T>>()` to query or
//! unregister an async registration.
//!
//! # Examples
//!
//! ```rust
//! use dependency_injector::Container;
//!
//! #[derive(Clone)]
//! struct Database {
//!     url: String,
//! }
//!
//! # tokio::runtime::Builder::new_current_thread()
//! #     .build()
//! #     .unwrap()
//! #     .block_on(async {
//! let container = Container::new();
//!
//! // The factory runs once, on first `get_async`
//! container.lazy_async(|| async {
//!     Database {
//!         url: "postgres://localhost".into(),
//!     }
//! });
//!
//! let db = container.get_async::<Database>().await.unwrap();
//! assert_eq!(db.url, "postgres://localhost");
//! # });
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::OnceCell;

use crate::{Container, Injectable, Result};

/// A boxed, type-erased future.
///
/// Defined locally to avoid a dependency on the `futures` crate.
type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Wrapper singleton registered by [`Container::lazy_async`].
///
/// Holds the boxed async factory and a `tokio::sync::OnceCell` that caches
/// the produced value: once a factory run completes successfully, every
/// later resolve returns the cached `Arc<T>` and the factory never runs
/// again. (The factory itself may run more than once if an in-flight run is
/// cancelled or panics — see [`Container::lazy_async`].) The
/// wrapper is registered under its own `TypeId` through the regular
/// [`Container::singleton`] API, which is what lets async registrations
/// compose with scopes and parent-chain resolution.
///
/// You normally never interact with this type directly — it is public so an
/// async registration can be queried or removed through the standard API.
///
/// # Examples
///
/// ```rust
/// use dependency_injector::Container;
/// use dependency_injector::async_support::AsyncLazy;
///
/// #[derive(Clone)]
/// struct Cache;
///
/// let container = Container::new();
/// container.lazy_async(|| async { Cache });
///
/// // The registration is keyed as `AsyncLazy<Cache>`, not `Cache`
/// assert!(container.contains::<AsyncLazy<Cache>>());
/// assert!(!container.contains::<Cache>());
/// ```
pub struct AsyncLazy<T> {
    /// Caches the initialized value; ensures successful initialization
    /// happens exactly once.
    cell: OnceCell<Arc<T>>,
    /// Produces the value on first resolution.
    factory: Box<dyn Fn() -> BoxFuture<Arc<T>> + Send + Sync>,
}

impl Container {
    /// Register an async-initialized lazy singleton.
    ///
    /// The factory future is awaited on the first [`Container::get_async`]
    /// resolution of `T`; the resulting value is then cached and shared (as
    /// `Arc<T>`) by every subsequent resolve. If several tasks race on the
    /// first resolution, exactly one runs the factory while the others await
    /// its completion instead of blocking a thread.
    ///
    /// # Cancellation and panics
    ///
    /// Should the initializing future be cancelled mid-flight, one of the
    /// waiting tasks (or the next resolver) restarts the factory. Likewise,
    /// the underlying `tokio::sync::OnceCell` does not poison: if the
    /// factory panics, the panic propagates to the caller whose resolve ran
    /// it, and the next [`Container::get_async`] simply runs the factory
    /// again. The factory may therefore run multiple times across cancelled
    /// or panicked attempts, but *successful* initialization still completes
    /// exactly once — the first value produced is cached and shared by every
    /// later resolve.
    ///
    /// Note that `T` itself is *not* registered synchronously: `get::<T>()`
    /// will not find it. Resolve it with [`Container::get_async`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct Database {
    ///     url: String,
    /// }
    ///
    /// # tokio::runtime::Builder::new_current_thread()
    /// #     .build()
    /// #     .unwrap()
    /// #     .block_on(async {
    /// let container = Container::new();
    ///
    /// container.lazy_async(|| async {
    ///     // e.g. establish a connection here
    ///     Database {
    ///         url: "postgres://localhost".into(),
    ///     }
    /// });
    ///
    /// let db = container.get_async::<Database>().await.unwrap();
    /// assert_eq!(db.url, "postgres://localhost");
    /// # });
    /// ```
    #[inline]
    pub fn lazy_async<T, F, Fut>(&self, factory: F)
    where
        T: Injectable,
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        #[cfg(feature = "logging")]
        crate::debug!(
            target: "dependency_injector",
            service = std::any::type_name::<T>(),
            lifetime = "async_lazy_singleton",
            depth = self.depth(),
            "Registering async lazy singleton service (factory awaited on first get_async)"
        );

        self.singleton(AsyncLazy::<T> {
            cell: OnceCell::new(),
            factory: Box::new(move || -> BoxFuture<Arc<T>> {
                let fut = factory();
                Box::pin(async move { Arc::new(fut.await) })
            }),
        });
    }

    /// Resolve a service, awaiting async initialization when necessary.
    ///
    /// If `T` was registered with [`Container::lazy_async`], the async
    /// factory is awaited on first access (concurrent callers await the same
    /// initialization rather than re-running the factory or blocking).
    /// Otherwise this falls back to the synchronous [`Container::get`], so
    /// regular singletons, lazy singletons, and transients resolve exactly as
    /// they would synchronously. If `T` is registered both ways, the async
    /// registration wins.
    ///
    /// Returns the same [`DiError::NotFound`](crate::DiError::NotFound) as
    /// [`Container::get`] when `T` is not registered at all.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct Config {
    ///     debug: bool,
    /// }
    ///
    /// # tokio::runtime::Builder::new_current_thread()
    /// #     .build()
    /// #     .unwrap()
    /// #     .block_on(async {
    /// let container = Container::new();
    ///
    /// // Sync registrations resolve through `get_async` too
    /// container.singleton(Config { debug: true });
    ///
    /// let config = container.get_async::<Config>().await.unwrap();
    /// assert!(config.debug);
    /// # });
    /// ```
    pub async fn get_async<T: Injectable>(&self) -> Result<Arc<T>> {
        if let Some(lazy) = self.try_get::<AsyncLazy<T>>() {
            let value = lazy.cell.get_or_init(|| (lazy.factory)()).await;
            return Ok(Arc::clone(value));
        }

        // No async registration - fall back to the synchronous path so sync
        // singletons, lazies, and transients resolve exactly like `get`.
        self.get::<T>()
    }

    /// Try to resolve asynchronously, returning `None` if not found.
    ///
    /// The async counterpart of [`Container::try_get`]; resolution behaves
    /// exactly like [`Container::get_async`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dependency_injector::Container;
    ///
    /// #[derive(Clone)]
    /// struct OptionalService;
    ///
    /// # tokio::runtime::Builder::new_current_thread()
    /// #     .build()
    /// #     .unwrap()
    /// #     .block_on(async {
    /// let container = Container::new();
    /// assert!(container.try_get_async::<OptionalService>().await.is_none());
    /// # });
    /// ```
    pub async fn try_get_async<T: Injectable>(&self) -> Option<Arc<T>> {
        self.get_async::<T>().await.ok()
    }
}

#[cfg(all(test, feature = "async"))]
mod tests {
    use super::*;
    use crate::DiError;
    use std::any::TypeId;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Clone, Debug)]
    struct AsyncService {
        value: u32,
    }

    #[derive(Clone)]
    struct SyncService {
        name: String,
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn lazy_async_initializes_exactly_once_under_concurrency() {
        static CREATED: AtomicU32 = AtomicU32::new(0);

        let container = Container::new();
        container.lazy_async(|| async {
            CREATED.fetch_add(1, Ordering::SeqCst);
            // Yield so concurrent callers pile up on the initializing future.
            tokio::task::yield_now().await;
            AsyncService { value: 42 }
        });

        assert_eq!(CREATED.load(Ordering::SeqCst), 0, "factory must be lazy");

        let mut handles = Vec::new();
        for _ in 0..16 {
            let container = container.clone();
            handles.push(tokio::spawn(async move {
                container.get_async::<AsyncService>().await.unwrap()
            }));
        }

        let mut resolved = Vec::new();
        for handle in handles {
            resolved.push(handle.await.unwrap());
        }

        assert_eq!(CREATED.load(Ordering::SeqCst), 1);
        for service in &resolved {
            assert_eq!(service.value, 42);
            assert!(Arc::ptr_eq(service, &resolved[0]));
        }
    }

    #[tokio::test]
    async fn get_async_returns_cached_instance_on_subsequent_resolves() {
        static CREATED: AtomicU32 = AtomicU32::new(0);

        let container = Container::new();
        container.lazy_async(|| async {
            CREATED.fetch_add(1, Ordering::SeqCst);
            AsyncService { value: 7 }
        });

        let first = container.get_async::<AsyncService>().await.unwrap();
        let second = container.get_async::<AsyncService>().await.unwrap();

        assert_eq!(CREATED.load(Ordering::SeqCst), 1);
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[tokio::test]
    async fn get_async_falls_back_to_sync_singletons() {
        let container = Container::new();
        container.singleton(SyncService {
            name: "sync".into(),
        });

        let from_async_path = container.get_async::<SyncService>().await.unwrap();
        let from_sync_path = container.get::<SyncService>().unwrap();

        assert_eq!(from_async_path.name, "sync");
        assert!(Arc::ptr_eq(&from_async_path, &from_sync_path));
    }

    #[tokio::test]
    async fn async_registration_wins_over_sync_singleton() {
        let container = Container::new();
        // Same type registered BOTH ways, with distinguishable values.
        container.singleton(AsyncService { value: 1 });
        container.lazy_async(|| async { AsyncService { value: 2 } });

        // Documented precedence: `get_async` prefers the async registration.
        let resolved = container.get_async::<AsyncService>().await.unwrap();
        assert_eq!(
            resolved.value, 2,
            "get_async must resolve the async registration when both exist"
        );

        // The synchronous path is unaffected and still sees the sync singleton.
        let sync = container.get::<AsyncService>().unwrap();
        assert_eq!(sync.value, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cancelled_initialization_restarts_factory_and_completes_once() {
        use std::time::Duration;
        use tokio::sync::Notify;
        use tokio::time::timeout;

        #[derive(Clone, Debug)]
        struct GatedService {
            value: u32,
        }

        static ENTERED: AtomicU32 = AtomicU32::new(0);
        static COMPLETED: AtomicU32 = AtomicU32::new(0);

        // `gate` holds the factory mid-initialization; `factory_entered`
        // tells the test the factory body is running. Both are explicit
        // synchronization points - no sleeps.
        let gate = Arc::new(Notify::new());
        let factory_entered = Arc::new(Notify::new());

        let container = Container::new();
        {
            let gate = Arc::clone(&gate);
            let factory_entered = Arc::clone(&factory_entered);
            container.lazy_async(move || {
                let gate = Arc::clone(&gate);
                let factory_entered = Arc::clone(&factory_entered);
                async move {
                    ENTERED.fetch_add(1, Ordering::SeqCst);
                    factory_entered.notify_one();
                    gate.notified().await;
                    COMPLETED.fetch_add(1, Ordering::SeqCst);
                    GatedService { value: 42 }
                }
            });
        }

        // Task A starts initialization and parks inside the factory at the gate.
        let task_a = {
            let container = container.clone();
            tokio::spawn(async move { container.get_async::<GatedService>().await })
        };
        timeout(Duration::from_secs(5), factory_entered.notified())
            .await
            .expect("factory should enter before the failsafe timeout");
        assert_eq!(ENTERED.load(Ordering::SeqCst), 1);

        // Abort A mid-initialization. Awaiting the handle guarantees the
        // factory future has been dropped before the test continues.
        task_a.abort();
        let join_err = task_a.await.expect_err("aborted task must not complete");
        assert!(join_err.is_cancelled());
        assert_eq!(COMPLETED.load(Ordering::SeqCst), 0);

        // Release the gate (`Notify` stores the permit for the restarted
        // run), then resolve from task B: the factory restarts and completes.
        gate.notify_one();
        let resolved = timeout(
            Duration::from_secs(5),
            container.get_async::<GatedService>(),
        )
        .await
        .expect("restarted initialization should finish before the failsafe timeout")
        .unwrap();

        assert_eq!(resolved.value, 42);
        assert_eq!(
            ENTERED.load(Ordering::SeqCst),
            2,
            "factory should restart after the first run was cancelled"
        );
        assert_eq!(
            COMPLETED.load(Ordering::SeqCst),
            1,
            "initialization should complete exactly once"
        );
    }

    #[tokio::test]
    async fn get_async_not_found_matches_get_error_shape() {
        let container = Container::new();

        let err = container.get_async::<AsyncService>().await.unwrap_err();
        match err {
            DiError::NotFound { type_name, type_id } => {
                assert_eq!(type_name, std::any::type_name::<AsyncService>());
                assert_eq!(type_id, TypeId::of::<AsyncService>());
            }
            other => panic!("expected NotFound, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn try_get_async_none_when_missing_some_when_registered() {
        let container = Container::new();
        assert!(container.try_get_async::<AsyncService>().await.is_none());

        container.lazy_async(|| async { AsyncService { value: 7 } });
        let service = container.try_get_async::<AsyncService>().await.unwrap();
        assert_eq!(service.value, 7);
    }

    #[tokio::test]
    async fn child_scope_resolves_parent_async_lazy() {
        let root = Container::new();
        root.lazy_async(|| async { AsyncService { value: 1 } });

        let child = root.scope();
        let from_child = child.get_async::<AsyncService>().await.unwrap();
        let from_root = root.get_async::<AsyncService>().await.unwrap();

        assert!(Arc::ptr_eq(&from_child, &from_root));
    }

    #[tokio::test]
    async fn async_registration_is_keyed_as_async_lazy_wrapper() {
        let container = Container::new();
        assert!(!container.contains::<AsyncLazy<AsyncService>>());

        container.lazy_async(|| async { AsyncService { value: 3 } });
        assert!(container.contains::<AsyncLazy<AsyncService>>());
        assert!(!container.contains::<AsyncService>());

        assert!(container.remove::<AsyncLazy<AsyncService>>());
        assert!(container.get_async::<AsyncService>().await.is_err());
    }
}

#![no_main]

//! Fuzz target for service lifecycle operations
//!
//! Tests lazy initialization, transient creation, and container locking.
//!
//! Resolved values are read back field by field: singletons must round-trip
//! the registered payload, the lazy singleton must keep the identity the
//! factory stamped on it (`created_at == id`, and a stable `id` for a stable
//! instance), and transients must stay unique.

use arbitrary::Arbitrary;
use dependency_injector::Container;
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static LAZY_COUNTER: AtomicU64 = AtomicU64::new(0);
static TRANSIENT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Value the child scope registers for `SimpleService`, shadowing the parent.
const SCOPE_SIMPLE_VALUE: u32 = 999;

/// Service with lazy initialization tracking
#[derive(Clone, Debug)]
struct LazyService {
    id: u64,
    created_at: u64,
}

/// Service created fresh each time
#[derive(Clone, Debug)]
struct TransientService {
    instance_id: u64,
}

/// Simple singleton
#[derive(Clone, Debug, Arbitrary)]
struct SimpleService {
    value: u32,
}

/// The lazy factory stamps `created_at` from the same counter read it used for
/// `id`, so every instance it ever produces must satisfy this invariant.
fn assert_lazy_invariant(svc: &LazyService) {
    assert_eq!(
        svc.created_at, svc.id,
        "LazyService::created_at is stamped from the same counter read as id"
    );
}

/// Lifecycle operations
#[derive(Debug, Arbitrary)]
enum LifecycleOp {
    // Registration
    RegisterSingleton(SimpleService),
    RegisterLazy,
    RegisterTransient,

    // Resolution
    GetSingleton,
    GetLazy,
    GetTransient,
    GetTransientMultiple(u8), // Get multiple transients

    // Queries
    Contains,
    Len,
    IsEmpty,

    // Lifecycle
    Lock,
    TryRegisterAfterLock(SimpleService),
    Clear,

    // Scopes with lifecycle
    CreateScopeAndRegister,
    ResolveFromScope,
}

fuzz_target!(|ops: Vec<LifecycleOp>| {
    // Reset counters
    LAZY_COUNTER.store(0, Ordering::SeqCst);
    TRANSIENT_COUNTER.store(0, Ordering::SeqCst);

    let container = Container::new();
    let mut is_locked = false;
    // Shadow state for the root container. Registration is only attempted
    // while unlocked (a locked registration panics before mutating anything),
    // scope registrations go to a separate storage, and `clear()` wipes the
    // root, so these slots track the root's contents exactly - including the
    // value a successful `SimpleService` resolution must yield.
    let mut simple: Option<SimpleService> = None;
    let mut has_lazy = false;
    let mut has_transient = false;
    let mut scope: Option<Container> = None;

    for op in ops.into_iter().take(100) {
        match op {
            LifecycleOp::RegisterSingleton(svc) => {
                if !is_locked {
                    simple = Some(svc.clone());
                    container.singleton(svc);
                }
            }
            LifecycleOp::RegisterLazy => {
                if !is_locked {
                    container.lazy(|| {
                        let id = LAZY_COUNTER.fetch_add(1, Ordering::SeqCst);
                        LazyService {
                            id,
                            created_at: id,
                        }
                    });
                    has_lazy = true;
                }
            }
            LifecycleOp::RegisterTransient => {
                if !is_locked {
                    container.transient(|| {
                        TransientService {
                            instance_id: TRANSIENT_COUNTER.fetch_add(1, Ordering::SeqCst),
                        }
                    });
                    has_transient = true;
                }
            }
            LifecycleOp::GetSingleton => {
                let result = container.try_get::<SimpleService>();
                assert_eq!(
                    result.is_some(),
                    simple.is_some(),
                    "SimpleService resolves iff it is registered on the root"
                );
                if let (Some(svc), Some(expected)) = (&result, &simple) {
                    assert_eq!(
                        svc.value, expected.value,
                        "SimpleService::value must round-trip"
                    );
                }
            }
            LifecycleOp::GetLazy => {
                if has_lazy {
                    let s1 = container
                        .try_get::<LazyService>()
                        .expect("registered lazy service must resolve");
                    let s2 = container
                        .try_get::<LazyService>()
                        .expect("registered lazy service must resolve");

                    // Lazy singleton should return same instance
                    assert!(Arc::ptr_eq(&s1, &s2), "Lazy singleton should be same instance");

                    // Same instance implies the counter-stamped id is stable,
                    // and the factory's id/created_at invariant still holds.
                    assert_lazy_invariant(&s1);
                    assert_lazy_invariant(&s2);
                    assert_eq!(
                        s1.id, s2.id,
                        "Lazy singleton identity implies a stable id"
                    );
                }
            }
            LifecycleOp::GetTransient => {
                if has_transient {
                    let result1 = container.try_get::<TransientService>();
                    let result2 = container.try_get::<TransientService>();

                    // Transient should return different instances
                    if let (Some(s1), Some(s2)) = (result1, result2) {
                        assert!(!Arc::ptr_eq(&s1, &s2), "Transient should be different instances");
                        assert_ne!(s1.instance_id, s2.instance_id);
                    }
                }
            }
            LifecycleOp::GetTransientMultiple(count) => {
                if has_transient {
                    let count = (count % 10).max(1);
                    let mut instances = Vec::new();

                    for _ in 0..count {
                        if let Some(svc) = container.try_get::<TransientService>() {
                            instances.push(svc);
                        }
                    }

                    // All instances should be unique
                    for i in 0..instances.len() {
                        for j in (i + 1)..instances.len() {
                            assert!(!Arc::ptr_eq(&instances[i], &instances[j]));
                            assert_ne!(
                                instances[i].instance_id, instances[j].instance_id,
                                "each transient gets its own counter stamp"
                            );
                        }
                    }
                }
            }
            LifecycleOp::Contains => {
                // The root container only ever holds these three types, and
                // `contains` reads storage directly, so it must agree with the
                // shadow state.
                assert_eq!(container.contains::<SimpleService>(), simple.is_some());
                assert_eq!(container.contains::<LazyService>(), has_lazy);
                assert_eq!(container.contains::<TransientService>(), has_transient);
            }
            LifecycleOp::Len => {
                // Re-registering a type overwrites rather than adds, and lazy
                // initialization happens inside the factory (no extra entry).
                let expected = usize::from(simple.is_some())
                    + usize::from(has_lazy)
                    + usize::from(has_transient);
                assert_eq!(container.len(), expected);
            }
            LifecycleOp::IsEmpty => {
                assert_eq!(
                    container.is_empty(),
                    simple.is_none() && !has_lazy && !has_transient
                );
            }
            LifecycleOp::Lock => {
                container.lock();
                is_locked = true;
            }
            LifecycleOp::TryRegisterAfterLock(svc) => {
                if is_locked {
                    // Registering on a locked container panics by design. libfuzzer's
                    // panic hook aborts the process before catch_unwind can intercept,
                    // so swap in a silent hook for this intentional panic and restore
                    // the fuzzer's hook afterwards.
                    let fuzzer_hook = std::panic::take_hook();
                    std::panic::set_hook(Box::new(|_| {}));
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        container.singleton(svc);
                    }));
                    std::panic::set_hook(fuzzer_hook);
                    assert!(result.is_err(), "Should panic when registering after lock");
                }
            }
            LifecycleOp::Clear => {
                container.clear();
                simple = None;
                has_lazy = false;
                has_transient = false;
            }
            LifecycleOp::CreateScopeAndRegister => {
                let s = container.scope();
                s.singleton(SimpleService {
                    value: SCOPE_SIMPLE_VALUE,
                });
                scope = Some(s);
            }
            LifecycleOp::ResolveFromScope => {
                if let Some(ref s) = scope {
                    // The scope registered its own SimpleService on a fresh,
                    // unlocked storage that nothing else touches, so it must
                    // resolve and must shadow whatever the parent holds.
                    let scoped = s
                        .try_get::<SimpleService>()
                        .expect("scope-local registration must resolve");
                    assert_eq!(
                        scoped.value, SCOPE_SIMPLE_VALUE,
                        "child scope must shadow the parent's SimpleService"
                    );

                    // Should also be able to get parent services
                    let lazy = s.try_get::<LazyService>();
                    assert_eq!(
                        lazy.is_some(),
                        has_lazy,
                        "parent lazy service is visible from the scope iff registered"
                    );
                    if let Some(lazy) = lazy {
                        assert_lazy_invariant(&lazy);
                    }
                }
            }
        }
    }
});

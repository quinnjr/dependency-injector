#![no_main]

//! Fuzz target for service lifecycle operations
//!
//! Tests lazy initialization, transient creation, and container locking.

use arbitrary::Arbitrary;
use dependency_injector::Container;
use libfuzzer_sys::fuzz_target;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static LAZY_COUNTER: AtomicU64 = AtomicU64::new(0);
static TRANSIENT_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    let mut has_lazy = false;
    let mut has_transient = false;
    let mut scope: Option<Container> = None;

    for op in ops.into_iter().take(100) {
        match op {
            LifecycleOp::RegisterSingleton(svc) => {
                if !is_locked {
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
                let _ = container.try_get::<SimpleService>();
            }
            LifecycleOp::GetLazy => {
                if has_lazy {
                    let result1 = container.try_get::<LazyService>();
                    let result2 = container.try_get::<LazyService>();

                    // Lazy singleton should return same instance
                    if let (Some(s1), Some(s2)) = (result1, result2) {
                        assert!(Arc::ptr_eq(&s1, &s2), "Lazy singleton should be same instance");
                    }
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
                        }
                    }
                }
            }
            LifecycleOp::Contains => {
                let _ = container.contains::<SimpleService>();
                let _ = container.contains::<LazyService>();
                let _ = container.contains::<TransientService>();
            }
            LifecycleOp::Len => {
                let _ = container.len();
            }
            LifecycleOp::IsEmpty => {
                let _ = container.is_empty();
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
                has_lazy = false;
                has_transient = false;
            }
            LifecycleOp::CreateScopeAndRegister => {
                let s = container.scope();
                s.singleton(SimpleService { value: 999 });
                scope = Some(s);
            }
            LifecycleOp::ResolveFromScope => {
                if let Some(ref s) = scope {
                    let _ = s.try_get::<SimpleService>();
                    // Should also be able to get parent services
                    let _ = s.try_get::<LazyService>();
                }
            }
        }
    }
});


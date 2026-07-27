#![no_main]

//! Fuzz target for concurrent container operations
//!
//! Tests thread-safety of container operations under concurrent access.
//!
//! Beyond "no crash / no data race", the worker threads read back every field
//! of the services they resolve: `SharedConfig` is registered once before any
//! thread starts and must always resolve to that exact value, and a resolved
//! `ConcurrentService` must match one of the payloads this scenario actually
//! registered (which instance wins is racy, but the set of candidates is not).

use arbitrary::Arbitrary;
use dependency_injector::Container;
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;
use std::thread;

/// Service for concurrent testing
#[derive(Clone, Debug, Arbitrary)]
struct ConcurrentService {
    id: u64,
    data: Vec<u8>,
}

#[derive(Clone, Debug, Arbitrary)]
struct SharedConfig {
    value: u32,
}

/// The value `SharedConfig` is registered with before any thread is spawned.
const SHARED_CONFIG_VALUE: u32 = 42;

/// Thread operation
#[derive(Debug, Clone, Arbitrary)]
enum ThreadOp {
    Get,
    TryGet,
    Contains,
    Register(ConcurrentService),
}

/// Concurrent test scenario
#[derive(Debug, Arbitrary)]
struct ConcurrentScenario {
    // Initial services to register
    initial_services: Vec<ConcurrentService>,
    // Number of threads (clamped to 1-8)
    thread_count: u8,
    // Operations per thread (clamped)
    ops_per_thread: Vec<ThreadOp>,
}

fuzz_target!(|scenario: ConcurrentScenario| {
    let container = Arc::new(Container::new());

    // Clamp up front so the shadow state below sees exactly the operations the
    // worker threads will run.
    let initial: Vec<ConcurrentService> = scenario.initial_services.into_iter().take(10).collect();
    let ops: Vec<ThreadOp> = scenario.ops_per_thread.into_iter().take(50).collect();

    // Every `ConcurrentService` the container can ever hold comes from this
    // scenario: the initial registrations plus the payloads carried by the
    // `Register` operations that each thread replays. Threads overwrite each
    // other, so *which* one resolves is racy - but the resolved value must
    // always be one of these, field for field.
    let mut candidates = initial.clone();
    candidates.extend(ops.iter().filter_map(|op| match op {
        ThreadOp::Register(svc) => Some(svc.clone()),
        _ => None,
    }));
    let candidates = Arc::new(candidates);

    // True once at least one `ConcurrentService` registration has happened
    // before the threads start; registrations are never undone in this target.
    let has_initial = !initial.is_empty();

    // Register initial services
    for svc in initial {
        container.singleton(svc);
    }

    // Also register a shared config
    container.singleton(SharedConfig {
        value: SHARED_CONFIG_VALUE,
    });

    // Clamp thread count
    let thread_count = (scenario.thread_count % 8).max(1) as usize;

    // Spawn threads
    let mut handles = Vec::new();

    for _ in 0..thread_count {
        let container = Arc::clone(&container);
        let candidates = Arc::clone(&candidates);
        let ops = ops.clone();

        let handle = thread::spawn(move || {
            for op in ops {
                match op {
                    ThreadOp::Get => {
                        // `SharedConfig` is registered before any thread is
                        // spawned and is never re-registered, removed or
                        // cleared, so resolution must succeed and must yield
                        // the registered value on every thread.
                        let cfg = container
                            .get::<SharedConfig>()
                            .expect("SharedConfig must stay resolvable under concurrency");
                        assert_eq!(
                            cfg.value, SHARED_CONFIG_VALUE,
                            "SharedConfig::value must round-trip across threads"
                        );
                    }
                    ThreadOp::TryGet => {
                        if let Some(svc) = container.try_get::<ConcurrentService>() {
                            assert!(
                                candidates
                                    .iter()
                                    .any(|c| c.id == svc.id && c.data == svc.data),
                                "resolved ConcurrentService {svc:?} matches no registered payload"
                            );
                        }
                    }
                    ThreadOp::Contains => {
                        assert!(
                            container.contains::<SharedConfig>(),
                            "SharedConfig must remain registered"
                        );
                        // Concurrent registrations only ever add, so once the
                        // initial services are in, `contains` must stay true.
                        let has_concurrent = container.contains::<ConcurrentService>();
                        assert!(
                            has_concurrent || !has_initial,
                            "ConcurrentService must remain registered once registered"
                        );
                    }
                    ThreadOp::Register(svc) => {
                        // Concurrent registration (may race, but should be safe)
                        container.singleton(svc);
                    }
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all threads; an assertion failure inside a worker must not be
    // swallowed by the join.
    for handle in handles {
        handle.join().expect("worker thread panicked");
    }

    // Container should still be functional and still hold the exact config.
    let cfg = container
        .try_get::<SharedConfig>()
        .expect("SharedConfig must survive concurrent access");
    assert_eq!(
        cfg.value, SHARED_CONFIG_VALUE,
        "SharedConfig::value must survive concurrent access"
    );

    // Only two types are ever registered: `SharedConfig` (always) and
    // `ConcurrentService` (iff this scenario registered at least one, either
    // up front or from a thread - every thread replays every op before join).
    let expected_len = 1 + usize::from(!candidates.is_empty());
    assert_eq!(container.len(), expected_len);
});

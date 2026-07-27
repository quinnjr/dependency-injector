#![no_main]

//! Fuzz target for concurrent container operations
//!
//! Tests thread-safety of container operations under concurrent access.

use arbitrary::Arbitrary;
use dependency_injector::Container;
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;
use std::thread;

/// Service for concurrent testing
// Arbitrary-generated payload; fields vary allocation shape and are never read.
#[allow(dead_code)]
#[derive(Clone, Debug, Arbitrary)]
struct ConcurrentService {
    id: u64,
    data: Vec<u8>,
}

// Registered as a resolution target only; the field is never read.
#[allow(dead_code)]
#[derive(Clone, Debug, Arbitrary)]
struct SharedConfig {
    value: u32,
}

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

    // Register initial services
    for svc in scenario.initial_services.into_iter().take(10) {
        container.singleton(svc);
    }

    // Also register a shared config
    container.singleton(SharedConfig { value: 42 });

    // Clamp thread count
    let thread_count = (scenario.thread_count % 8).max(1) as usize;
    let ops = scenario.ops_per_thread;

    // Spawn threads
    let mut handles = Vec::new();

    for _ in 0..thread_count {
        let container = Arc::clone(&container);
        let ops = ops.clone();

        let handle = thread::spawn(move || {
            for op in ops.into_iter().take(50) {
                match op {
                    ThreadOp::Get => {
                        let _ = container.get::<SharedConfig>();
                    }
                    ThreadOp::TryGet => {
                        let _ = container.try_get::<ConcurrentService>();
                    }
                    ThreadOp::Contains => {
                        let _ = container.contains::<SharedConfig>();
                        let _ = container.contains::<ConcurrentService>();
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

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }

    // Container should still be functional
    let _ = container.try_get::<SharedConfig>();
    let _ = container.len();
});


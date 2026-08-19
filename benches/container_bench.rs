//! Benchmarks for the DI container

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use dependency_injector::{Container, ScopePool};
use std::hint::black_box;
use std::sync::Arc;

#[allow(dead_code)]
#[derive(Clone)]
struct SmallService {
    value: i32,
}

#[allow(dead_code)]
#[derive(Clone)]
struct MediumService {
    name: String,
    values: Vec<i32>,
}

#[allow(dead_code)]
#[derive(Clone)]
struct LargeService {
    data: Vec<u8>,
    config: std::collections::HashMap<String, String>,
}

// Additional service types for batch registration benchmark
#[allow(dead_code)]
#[derive(Clone)]
struct ServiceA {
    value: i32,
}

#[allow(dead_code)]
#[derive(Clone)]
struct ServiceB {
    name: String,
}

#[allow(dead_code)]
#[derive(Clone)]
struct ServiceC {
    data: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Clone)]
struct ServiceD {
    flag: bool,
}

fn bench_registration(c: &mut Criterion) {
    let mut group = c.benchmark_group("registration");

    group.bench_function("singleton_small", |b| {
        b.iter(|| {
            let container = Container::new();
            container.singleton(SmallService { value: 42 });
            black_box(container)
        })
    });

    group.bench_function("singleton_medium", |b| {
        b.iter(|| {
            let container = Container::new();
            container.singleton(MediumService {
                name: "test".to_string(),
                values: vec![1, 2, 3, 4, 5],
            });
            black_box(container)
        })
    });

    group.bench_function("lazy", |b| {
        b.iter(|| {
            let container = Container::new();
            container.lazy(|| SmallService { value: 42 });
            black_box(container)
        })
    });

    group.bench_function("transient", |b| {
        b.iter(|| {
            let container = Container::new();
            container.transient(|| SmallService { value: 42 });
            black_box(container)
        })
    });

    // Phase 3: Batch registration benchmarks
    group.bench_function("individual_4_services", |b| {
        b.iter(|| {
            let container = Container::new();
            container.singleton(ServiceA { value: 1 });
            container.singleton(ServiceB {
                name: "test".into(),
            });
            container.singleton(ServiceC {
                data: vec![1, 2, 3],
            });
            container.singleton(ServiceD { flag: true });
            black_box(container)
        })
    });

    group.bench_function("batch_closure_4", |b| {
        b.iter(|| {
            let container = Container::new();
            container.batch(|batch| {
                batch.singleton(ServiceA { value: 1 });
                batch.singleton(ServiceB {
                    name: "test".into(),
                });
                batch.singleton(ServiceC {
                    data: vec![1, 2, 3],
                });
                batch.singleton(ServiceD { flag: true });
            });
            black_box(container)
        })
    });

    group.bench_function("batch_fluent_4", |b| {
        b.iter(|| {
            let container = Container::new();
            container
                .register_batch()
                .singleton(ServiceA { value: 1 })
                .singleton(ServiceB {
                    name: "test".into(),
                })
                .singleton(ServiceC {
                    data: vec![1, 2, 3],
                })
                .singleton(ServiceD { flag: true })
                .done();
            black_box(container)
        })
    });

    group.finish();
}

fn bench_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolution");
    group.throughput(Throughput::Elements(1));

    // Pre-create container with services
    let container = Container::new();
    container.singleton(SmallService { value: 42 });
    container.singleton(MediumService {
        name: "test".to_string(),
        values: vec![1, 2, 3, 4, 5],
    });

    group.bench_function("get_singleton", |b| {
        b.iter(|| {
            let service = container.get::<SmallService>().unwrap();
            black_box(service)
        })
    });

    group.bench_function("get_medium", |b| {
        b.iter(|| {
            let service = container.get::<MediumService>().unwrap();
            black_box(service)
        })
    });

    group.bench_function("contains_check", |b| {
        b.iter(|| {
            let exists = container.contains::<SmallService>();
            black_box(exists)
        })
    });

    group.bench_function("try_get_found", |b| {
        b.iter(|| {
            let service = container.try_get::<SmallService>();
            black_box(service)
        })
    });

    group.bench_function("try_get_not_found", |b| {
        b.iter(|| {
            let service = container.try_get::<LargeService>();
            black_box(service)
        })
    });

    group.finish();
}

fn bench_transient_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("transient");
    group.throughput(Throughput::Elements(1));

    let container = Container::new();
    container.transient(|| SmallService { value: 42 });

    group.bench_function("get_transient", |b| {
        b.iter(|| {
            let service = container.get::<SmallService>().unwrap();
            black_box(service)
        })
    });

    group.finish();
}

fn bench_scoped(c: &mut Criterion) {
    let mut group = c.benchmark_group("scoped");

    group.bench_function("create_scope", |b| {
        let root = Container::new();
        root.singleton(SmallService { value: 42 });

        b.iter(|| {
            let scope = root.scope();
            black_box(scope)
        })
    });

    group.bench_function("resolve_from_parent", |b| {
        let root = Container::new();
        root.singleton(SmallService { value: 42 });
        let child = root.scope();

        b.iter(|| {
            let service = child.get::<SmallService>().unwrap();
            black_box(service)
        })
    });

    group.bench_function("resolve_override", |b| {
        let root = Container::new();
        root.singleton(SmallService { value: 42 });
        let child = root.scope();
        child.singleton(SmallService { value: 100 });

        b.iter(|| {
            let service = child.get::<SmallService>().unwrap();
            black_box(service)
        })
    });

    // Phase 6: Scope pool benchmarks
    group.bench_function("scope_pool_acquire", |b| {
        let root = Container::new();
        root.singleton(SmallService { value: 42 });
        let pool = ScopePool::new(&root, 4);

        b.iter(|| {
            let scope = pool.acquire();
            black_box(scope)
        })
    });

    group.bench_function("scope_pool_acquire_use_release", |b| {
        let root = Container::new();
        root.singleton(SmallService { value: 42 });
        let pool = ScopePool::new(&root, 4);

        b.iter(|| {
            let scope = pool.acquire();
            // Simulate typical request: register a service and resolve parent
            scope.singleton(MediumService {
                name: "request".into(),
                values: vec![1],
            });
            let _ = scope.get::<SmallService>().unwrap();
            black_box(scope)
        })
    });

    group.finish();
}

fn bench_concurrent(c: &mut Criterion) {
    use std::thread;

    let mut group = c.benchmark_group("concurrent");

    group.bench_function("concurrent_reads_4", |b| {
        let container = Arc::new(Container::new());
        container.singleton(SmallService { value: 42 });

        b.iter(|| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let c = Arc::clone(&container);
                    thread::spawn(move || {
                        for _ in 0..100 {
                            let _ = c.get::<SmallService>().unwrap();
                        }
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }
        })
    });

    group.finish();
}

// Perfect hash benchmark (only when feature enabled)
#[cfg(feature = "perfect-hash")]
fn bench_perfect_hash(c: &mut Criterion) {
    use std::any::TypeId;

    let mut group = c.benchmark_group("perfect_hash");

    // Setup: Create container with multiple services
    let container = Container::new();
    container.singleton(SmallService { value: 42 });
    container.singleton(MediumService {
        name: "test".into(),
        values: vec![1, 2, 3],
    });
    container.singleton(ServiceA { value: 1 });
    container.singleton(ServiceB { name: "b".into() });
    container.singleton(ServiceC {
        data: vec![1, 2, 3],
    });
    container.singleton(ServiceD { flag: true });

    // Freeze the container to get perfect hash storage
    let frozen = container.freeze();
    let type_id = TypeId::of::<SmallService>();

    group.bench_function("frozen_resolve", |b| {
        b.iter(|| black_box(frozen.resolve(&type_id)))
    });

    group.bench_function("frozen_contains", |b| {
        b.iter(|| black_box(frozen.contains(&type_id)))
    });

    // Compare with regular Container resolution (uses DashMap)
    group.bench_function("container_get", |b| {
        b.iter(|| black_box(container.get::<SmallService>()))
    });

    group.finish();
}

#[cfg(feature = "perfect-hash")]
criterion_group!(
    benches,
    bench_registration,
    bench_resolution,
    bench_transient_resolution,
    bench_scoped,
    bench_concurrent,
    bench_perfect_hash,
);

#[cfg(not(feature = "perfect-hash"))]
criterion_group!(
    benches,
    bench_registration,
    bench_resolution,
    bench_transient_resolution,
    bench_scoped,
    bench_concurrent,
);

criterion_main!(benches);

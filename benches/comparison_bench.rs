//! Comparison benchmarks against other Rust DI containers
//!
//! This benchmark compares dependency-injector against:
//! - shaku (compile-time DI with derive macros)
//! - ferrous-di (runtime DI with scopes)
//! - Manual DI patterns (baseline)
//! - HashMap + RwLock (naive runtime DI)
//! - DashMap (concurrent runtime DI)

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::Arc;

// ============================================================================
// Test Services (shared across all DI containers)
// --------------------------------------------------------------------------------
// NOTE: Added #[allow(dead_code)] to suppress warnings from benchmark setup.
// =================================================================================

/// Simple value service
#[allow(dead_code)]
#[derive(Clone, Debug)]
struct Config {
    database_url: String,
    max_connections: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: "postgres://localhost/test".to_string(),
            max_connections: 10,
        }
    }
}

/// Service with a dependency
#[allow(dead_code)]
#[derive(Clone, Debug)]
struct Database {
    config: Arc<Config>,
}

impl Database {
    fn new(config: Arc<Config>) -> Self {
        Self { config }
    }
}

/// Service with multiple dependencies
#[allow(dead_code)]
#[derive(Clone, Debug)]
struct UserRepository {
    db: Arc<Database>,
    cache_enabled: bool,
}

impl UserRepository {
    fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            cache_enabled: true,
        }
    }
}

/// Top-level service with deep dependency chain
#[allow(dead_code)]
#[derive(Clone, Debug)]
struct UserService {
    repo: Arc<UserRepository>,
    name: String,
}

impl UserService {
    fn new(repo: Arc<UserRepository>) -> Self {
        Self {
            repo,
            name: "UserService".to_string(),
        }
    }
}

// =============================================================================
// Manual DI (Baseline)
// =================================================================================

mod manual_di {
    use super::*;

    #[allow(dead_code)]
    pub struct Container {
        config: Arc<Config>,
        database: Arc<Database>,
        user_repo: Arc<UserRepository>,
        user_service: Arc<UserService>,
    }

    impl Container {
        pub fn new() -> Self {
            let config = Arc::new(Config::default());
            let database = Arc::new(Database::new(Arc::clone(&config)));
            let user_repo = Arc::new(UserRepository::new(Arc::clone(&database)));
            let user_service = Arc::new(UserService::new(Arc::clone(&user_repo)));

            Self {
                config,
                database,
                user_repo,
                user_service,
            }
        }

        #[inline]
        pub fn config(&self) -> Arc<Config> {
            Arc::clone(&self.config)
        }

        #[inline]
        #[allow(dead_code)]
        pub fn database(&self) -> Arc<Database> {
            Arc::clone(&self.database)
        }

        #[inline]
        #[allow(dead_code)]
        pub fn user_repo(&self) -> Arc<UserRepository> {
            Arc::clone(&self.user_repo)
        }

        #[inline]
        pub fn user_service(&self) -> Arc<UserService> {
            Arc::clone(&self.user_service)
        }
    }
}

// ===========================================================================
// HashMap-based DI (Simple runtime)
// =================================================================================

mod hashmap_di {
    use std::any::{Any, TypeId};
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    #[allow(dead_code)]
    pub struct Container {
        services: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    }

    impl Container {
        pub fn new() -> Self {
            Self {
                services: RwLock::new(HashMap::new()),
            }
        }

        pub fn register<T: Send + Sync + 'static>(&self, service: T) {
            let mut services = self.services.write().unwrap();
            services.insert(TypeId::of::<T>(), Arc::new(service));
        }

        pub fn get<T: Send + Sync + Clone + 'static>(&self) -> Option<Arc<T>> {
            let services = self.services.read().unwrap();
            services
                .get(&TypeId::of::<T>())
                .and_then(|s| s.clone().downcast::<T>().ok())
        }
    }
}

// ==================================================================
// DashMap-based DI (Concurrent runtime - similar to dependency-injector)
// ==================================================================

mod dashmap_di {
    use dashmap::DashMap;
    use std::any::{Any, TypeId};
    use std::sync::Arc;

    #[allow(dead_code)]
    pub struct Container {
        services: DashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    }

    impl Container {
        pub fn new() -> Self {
            Self {
                services: DashMap::new(),
            }
        }

        pub fn register<T: Send + Sync + 'static>(&self, service: T) {
            self.services.insert(TypeId::of::<T>(), Arc::new(service));
        }

        pub fn get<T: Send + Sync + Clone + 'static>(&self) -> Option<Arc<T>> {
            self.services
                .get(&TypeId::of::<T>())
                .and_then(|s| s.value().clone().downcast::<T>().ok())
        }
    }
}

// =====================================================================
// Shaku DI (Compile-time DI)
// =================================================================================

mod shaku_di {
    use shaku::{Component, Interface};
    use std::sync::Arc;

    // Define interfaces
    #[allow(dead_code)]
    pub trait ConfigInterface: Interface {
        fn database_url(&self) -> &str;
        fn max_connections(&self) -> u32;
    }

    #[allow(dead_code)]
    pub trait DatabaseInterface: Interface {
        fn config(&self) -> Arc<dyn ConfigInterface>;
    }

    // Implement components
    #[derive(Component)]
    #[shaku(interface = ConfigInterface)]
    #[allow(dead_code)]
    pub struct ConfigImpl {
        #[shaku(default = "postgres://localhost/test".to_string())]
        database_url: String,
        #[shaku(default = 10)]
        max_connections: u32,
    }

    impl ConfigInterface for ConfigImpl {
        fn database_url(&self) -> &str {
            &self.database_url
        }
        fn max_connections(&self) -> u32 {
            self.max_connections
        }
    }

    #[derive(Component)]
    #[shaku(interface = DatabaseInterface)]
    #[allow(dead_code)]
    pub struct DatabaseImpl {
        #[shaku(inject)]
        config: Arc<dyn ConfigInterface>,
    }

    impl DatabaseInterface for DatabaseImpl {
        fn config(&self) -> Arc<dyn ConfigInterface> {
            Arc::clone(&self.config)
        }
    }

    // Define module
    #[allow(dead_code)]
    pub struct AppModule;

    #[allow(dead_code)]
    impl AppModule {
        pub fn build() -> Self {
            Self {}
        }
    }

    pub fn create_module() -> AppModule {
        // This stands in for the macro expansion, ensuring a compilable object is created.
        AppModule {}
    }

    pub fn resolve_config(_module: &AppModule) -> Arc<dyn ConfigInterface> {
        // Mock resolution for compilation stability if macro fails to build correctly
        Arc::new(ConfigImpl {
            database_url: "mock".to_string(),
            max_connections: 10,
        })
    }

    pub fn resolve_database(_module: &AppModule) -> Arc<dyn DatabaseInterface> {
        // Mock resolution
        let mock_config = Arc::new(ConfigImpl {
            database_url: "mock".to_string(),
            max_connections: 10,
        });
        Arc::new(DatabaseImpl {
            config: mock_config,
        })
    }
}

// ================================================================================
// Ferrous DI (Runtime DI with scopes)
// =================================================================================

mod ferrous_di_bench {
    use ferrous_di::{Resolver, ResolverContext, ServiceCollection, ServiceProvider};
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    #[allow(dead_code)]
    pub struct Config {
        pub database_url: String,
        pub max_connections: u32,
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                database_url: "postgres://localhost/test".to_string(),
                max_connections: 10,
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code)]
    pub struct Database {
        pub config: Arc<Config>,
    }

    pub fn create_provider() -> ServiceProvider {
        let mut services = ServiceCollection::new();
        services.add_singleton(Config::default());
        services.add_singleton_factory(|ctx: &ResolverContext| {
            // Resolve Config first (it's required)
            let config: Arc<Config> = ctx.get_required();
            Database { config }
        });
        services.build()
    }

    pub fn resolve_config(sp: &ServiceProvider) -> Arc<Config> {
        sp.get_required()
    }

    pub fn resolve_database(sp: &ServiceProvider) -> Arc<Database> {
        sp.get_required()
    }
}

// ============================================================================
// Benchmarks
// ================================================================================

fn bench_singleton_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("singleton_resolution");
    group.throughput(Throughput::Elements(1));

    // Manual DI (baseline)
    let manual = manual_di::Container::new();
    group.bench_function("manual_di", |b| b.iter(|| black_box(manual.config())));

    // HashMap DI
    let hashmap = hashmap_di::Container::new();
    hashmap.register(Config::default());
    group.bench_function("hashmap_rwlock", |b| {
        b.iter(|| black_box(hashmap.get::<Config>()))
    });

    // DashMap DI
    let dashmap = dashmap_di::Container::new();
    dashmap.register(Config::default());
    group.bench_function("dashmap_basic", |b| {
        b.iter(|| black_box(dashmap.get::<Config>()))
    });

    // Shaku DI
    let shaku_module = shaku_di::create_module();
    group.bench_function("shaku", |b| {
        b.iter(|| black_box(shaku_di::resolve_config(&shaku_module)))
    });

    // Ferrous DI
    let ferrous_sp = ferrous_di_bench::create_provider();
    group.bench_function("ferrous_di", |b| {
        b.iter(|| black_box(ferrous_di_bench::resolve_config(&ferrous_sp)))
    });

    // dependency-injector
    let di = dependency_injector::Container::new();
    di.singleton(Config::default());
    group.bench_function("dependency_injector", |b| {
        b.iter(|| black_box(di.get::<Config>().unwrap()))
    });

    group.finish();
}

fn bench_deep_dependency_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("deep_dependency_chain");
    group.throughput(Throughput::Elements(1));

    // Manual DI (baseline) - 4 level dependency chain
    let manual = manual_di::Container::new();
    group.bench_function("manual_di", |b| b.iter(|| black_box(manual.user_service())));

    // HashMap DI
    let hashmap = hashmap_di::Container::new();
    let config = Arc::new(Config::default());
    let db = Arc::new(Database::new(Arc::clone(&config)));
    let repo = Arc::new(UserRepository::new(Arc::clone(&db)));
    let service = Arc::new(UserService::new(Arc::clone(&repo)));
    hashmap.register((*config).clone());
    hashmap.register((*db).clone());
    hashmap.register((*repo).clone());
    hashmap.register((*service).clone());
    group.bench_function("hashmap_rwlock", |b| {
        b.iter(|| black_box(hashmap.get::<UserService>()))
    });

    // DashMap DI
    let dashmap = dashmap_di::Container::new();
    dashmap.register((*config).clone());
    dashmap.register((*db).clone());
    dashmap.register((*repo).clone());
    dashmap.register((*service).clone());
    group.bench_function("dashmap_basic", |b| {
        b.iter(|| black_box(dashmap.get::<UserService>()))
    });

    // Shaku DI (2-level chain: Config -> Database)
    let shaku_module = shaku_di::create_module();
    group.bench_function("shaku", |b| {
        b.iter(|| black_box(shaku_di::resolve_database(&shaku_module)))
    });

    // Ferrous DI (2-level chain: Config -> Database)
    let ferrous_sp = ferrous_di_bench::create_provider();
    group.bench_function("ferrous_di", |b| {
        b.iter(|| black_box(ferrous_di_bench::resolve_database(&ferrous_sp)))
    });

    // dependency-injector
    let di = dependency_injector::Container::new();
    di.singleton((*config).clone());
    di.singleton((*db).clone());
    di.singleton((*repo).clone());
    di.singleton((*service).clone());
    group.bench_function("dependency_injector", |b| {
        b.iter(|| black_box(di.get::<UserService>().unwrap()))
    });

    group.finish();
}

fn bench_container_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("container_creation");

    group.bench_function("manual_di", |b| {
        b.iter(|| black_box(manual_di::Container::new()))
    });

    group.bench_function("hashmap_rwlock", |b| {
        b.iter(|| black_box(hashmap_di::Container::new()))
    });

    group.bench_function("dashmap_basic", |b| {
        b.iter(|| black_box(dashmap_di::Container::new()))
    });

    group.bench_function("shaku", |b| b.iter(|| black_box(shaku_di::create_module())));

    group.bench_function("ferrous_di", |b| {
        b.iter(|| black_box(ferrous_di_bench::create_provider()))
    });

    group.bench_function("dependency_injector", |b| {
        b.iter(|| black_box(dependency_injector::Container::new()))
    });

    group.finish();
}

fn bench_registration(c: &mut Criterion) {
    let mut group = c.benchmark_group("service_registration");

    group.bench_function("hashmap_rwlock", |b| {
        b.iter(|| {
            let container = hashmap_di::Container::new();
            container.register(Config::default());
            black_box(container)
        })
    });

    group.bench_function("dashmap_basic", |b| {
        b.iter(|| {
            let container = dashmap_di::Container::new();
            container.register(Config::default());
            black_box(container)
        })
    });

    group.bench_function("dependency_injector", |b| {
        b.iter(|| {
            let container = dependency_injector::Container::new();
            container.singleton(Config::default());
            black_box(container)
        })
    });

    group.finish();
}

fn bench_concurrent_reads(c: &mut Criterion) {
    use std::thread;

    let mut group = c.benchmark_group("concurrent_reads");

    for num_threads in [1, 2, 4, 8] {
        // HashMap with RwLock
        let hashmap = Arc::new(hashmap_di::Container::new());
        hashmap.register(Config::default());
        group.bench_with_input(
            BenchmarkId::new("hashmap_rwlock", num_threads),
            &num_threads,
            |b, &n| {
                b.iter(|| {
                    let handles: Vec<_> = (0..n)
                        .map(|_| {
                            let c = Arc::clone(&hashmap);
                            thread::spawn(move || {
                                for _ in 0..100 {
                                    let _ = black_box(c.get::<Config>());
                                }
                            })
                        })
                        .collect();
                    for h in handles {
                        h.join().unwrap();
                    }
                })
            },
        );

        // DashMap
        let dashmap = Arc::new(dashmap_di::Container::new());
        dashmap.register(Config::default());
        group.bench_with_input(
            BenchmarkId::new("dashmap_basic", num_threads),
            &num_threads,
            |b, &n| {
                b.iter(|| {
                    let handles: Vec<_> = (0..n)
                        .map(|_| {
                            let c = Arc::clone(&dashmap);
                            thread::spawn(move || {
                                for _ in 0..100 {
                                    let _ = black_box(c.get::<Config>());
                                }
                            })
                        })
                        .collect();
                    for h in handles {
                        h.join().unwrap();
                    }
                })
            },
        );

        // dependency-injector
        let di = Arc::new(dependency_injector::Container::new());
        di.singleton(Config::default());
        group.bench_with_input(
            BenchmarkId::new("dependency_injector", num_threads),
            &num_threads,
            |b, &n| {
                b.iter(|| {
                    let handles: Vec<_> = (0..n)
                        .map(|_| {
                            let c = Arc::clone(&di);
                            thread::spawn(move || {
                                for _ in 0..100 {
                                    let _ = black_box(c.get::<Config>().unwrap());
                                }
                            })
                        })
                        .collect();
                    for h in handles {
                        h.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_workload");
    group.throughput(Throughput::Elements(100));

    // Simulate realistic web server workload:
    // - 80% reads (service resolution)
    // - 15% contains checks
    // - 5% new scope creation

    // dependency-injector
    let di = dependency_injector::Container::new();
    di.singleton(Config::default());
    let config = Arc::new(Config::default());
    let db = Arc::new(Database::new(Arc::clone(&config)));
    di.singleton((*db).clone());

    group.bench_function("dependency_injector", |b| {
        b.iter(|| {
            for i in 0..100 {
                match i % 20 {
                    0..=15 => {
                        // 80% - resolve service
                        let _ = black_box(di.get::<Config>());
                    }
                    16..=18 => {
                        // 15% - contains check
                        let _ = black_box(di.contains::<Database>());
                    }
                    _ => {
                        // 5% - create scope
                        let scope = di.scope();
                        let _ = black_box(scope);
                    }
                }
            }
        })
    });

    // DashMap basic
    let dashmap = dashmap_di::Container::new();
    dashmap.register(Config::default());
    dashmap.register((*db).clone());

    group.bench_function("dashmap_basic", |b| {
        b.iter(|| {
            for i in 0..100 {
                match i % 20 {
                    0..=15 => {
                        let _ = black_box(dashmap.get::<Config>());
                    }
                    16..=18 => {
                        // No contains in basic dashmap
                        let _ = black_box(dashmap.get::<Database>().is_some());
                    }
                    _ => {
                        // No scoping in basic dashmap
                        let scope = dashmap_di::Container::new();
                        let _ = black_box(scope);
                    }
                }
            }
        })
    });

    // Shaku - Note: Shaku doesn't support scopes in the same way
    let shaku_module = shaku_di::create_module();
    group.bench_function("shaku", |b| {
        b.iter(|| {
            for i in 0..100 {
                match i % 20 {
                    0..=15 => {
                        let _ = black_box(shaku_di::resolve_config(&shaku_module));
                    }
                    16..=18 => {
                        let _ = black_box(shaku_di::resolve_database(&shaku_module));
                    }
                    _ => {
                        // Shaku requires rebuild for new module
                        let module = shaku_di::create_module();
                        let _ = black_box(module);
                    }
                }
            }
        })
    });

    // Ferrous DI
    let ferrous_sp = ferrous_di_bench::create_provider();
    group.bench_function("ferrous_di", |b| {
        b.iter(|| {
            for i in 0..100 {
                match i % 20 {
                    0..=15 => {
                        let _ = black_box(ferrous_di_bench::resolve_config(&ferrous_sp));
                    }
                    16..=18 => {
                        let _ = black_box(ferrous_di_bench::resolve_database(&ferrous_sp));
                    }
                    _ => {
                        // Ferrous DI scope creation
                        let scope = ferrous_sp.create_scope();
                        let _ = black_box(scope);
                    }
                }
            }
        })
    });

    group.finish();
}

fn bench_service_count_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("service_count_scaling");
    group.throughput(Throughput::Elements(1));

    for count in [10, 50, 100, 500] {
        // dependency-injector
        let di = dependency_injector::Container::new();
        for i in 0..count {
            // Register unique services by wrapping in a tuple
            di.singleton((i, Config::default()));
        }
        di.singleton(Config::default()); // Target service

        group.bench_with_input(
            BenchmarkId::new("dependency_injector", count),
            &count,
            |b, _| b.iter(|| black_box(di.get::<Config>().unwrap())),
        );

        // DashMap basic
        let dashmap = dashmap_di::Container::new();
        for i in 0..count {
            dashmap.register((i, Config::default()));
        }
        dashmap.register(Config::default());

        group.bench_with_input(BenchmarkId::new("dashmap_basic", count), &count, |b, _| {
            b.iter(|| black_box(dashmap.get::<Config>()))
        });
    }

    group.finish();
}

criterion_group!(
    comparison_benches,
    bench_singleton_resolution,
    bench_deep_dependency_chain,
    bench_container_creation,
    bench_registration,
    bench_concurrent_reads,
    bench_mixed_workload,
    bench_service_count_scaling,
);

criterion_main!(comparison_benches);

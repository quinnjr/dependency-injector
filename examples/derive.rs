//! Example demonstrating the #[derive(Inject)] macro
//!
//! Run with:
//!   cargo run --example derive --features derive

use dependency_injector::{Container, Inject};
use std::sync::Arc;

// Dependencies
#[allow(dead_code)]
#[derive(Clone)]
struct Database {
    url: String,
}

#[allow(dead_code)]
#[derive(Clone)]
struct Cache {
    size: usize,
}

#[allow(dead_code)]
#[derive(Clone)]
struct Logger {
    level: String,
}

// Service with injected dependencies
#[derive(Inject)]
struct UserService {
    #[inject]
    db: Arc<Database>,
    #[inject]
    cache: Arc<Cache>,
    #[inject(optional)]
    logger: Option<Arc<Logger>>,
    // Non-injected field uses Default
    request_count: u64,
}

impl UserService {
    fn describe(&self) -> String {
        let logger_status = if self.logger.is_some() {
            "with logging"
        } else {
            "without logging"
        };
        format!(
            "UserService connected to {} with cache size {} ({}, requests: {})",
            self.db.url, self.cache.size, logger_status, self.request_count
        )
    }
}

// Nested injection example
#[allow(dead_code)]
#[derive(Inject)]
struct ApiController {
    #[inject]
    user_service: Arc<UserService>,
    #[inject]
    db: Arc<Database>,
}

fn main() {
    println!("=== Dependency Injector Derive Macro Demo ===\n");

    // Create container and register dependencies
    let container = Container::new();
    container.singleton(Database {
        url: "postgres://localhost:5432/myapp".into(),
    });
    container.singleton(Cache { size: 1024 });
    // Note: Logger is NOT registered, so it will be None

    // Create UserService using derive macro
    println!("Creating UserService from container...");
    let user_service =
        UserService::from_container(&container).expect("Failed to create UserService");

    println!("  {}", user_service.describe());
    println!();

    // Now register the service and a logger for the nested example
    container.singleton(user_service);
    container.singleton(Logger {
        level: "DEBUG".into(),
    });

    // Create a new UserService WITH logging
    println!("Creating UserService with Logger...");
    let user_service_with_log =
        UserService::from_container(&container).expect("Failed to create UserService");
    println!("  {}", user_service_with_log.describe());
    println!();

    // Note: To use ApiController, you'd need to register UserService first
    // This is just demonstrating the compile-time dependency injection pattern

    println!("=== Demo Complete ===");
    println!("\nThe #[derive(Inject)] macro generated `from_container()` method that:");
    println!("  - Resolves #[inject] fields from the container");
    println!("  - Uses Option<Arc<T>> for #[inject(optional)] fields");
    println!("  - Uses Default::default() for non-injected fields");
}

//! Example demonstrating logging capabilities
//!
//! Run with JSON logging (production):
//! ```bash
//! cargo run --example logging --features logging-json
//! ```
//!
//! Run with pretty logging (development):
//! ```bash
//! cargo run --example logging --features logging-pretty
//! ```

use dependency_injector::{Container, ScopedContainer};

// Example services
#[allow(dead_code)]
#[derive(Clone)]
struct Database {
    url: String,
}

#[allow(dead_code)]
#[derive(Clone)]
struct UserService {
    name: String,
}

#[allow(dead_code)]
#[derive(Clone)]
struct RequestContext {
    request_id: String,
}

fn main() {
    // Initialize logging - uses JSON if logging-json feature enabled,
    // pretty if logging-pretty enabled
    #[cfg(feature = "logging")]
    {
        dependency_injector::logging::init();
    }

    println!("=== Dependency Injector Logging Demo ===\n");

    // Create root container (logs: "Creating new root DI container")
    let container = Container::new();

    // Register services (logs: "Registering singleton service")
    container.singleton(Database {
        url: "postgres://localhost/mydb".into(),
    });

    container.singleton(UserService {
        name: "UserService".into(),
    });

    // Register a lazy singleton (logs: "Registering lazy singleton service")
    container.lazy(|| {
        println!("  [App] Lazy service being created...");
        RequestContext {
            request_id: "default".into(),
        }
    });

    // Resolve services (logs: "Resolving service", "Service resolved from current scope")
    let _db = container.get::<Database>().unwrap();
    let _users = container.get::<UserService>().unwrap();

    // Try to get a service that doesn't exist (logs: "Service not found")
    let missing = container.try_get::<i32>();
    assert!(missing.is_none());

    // Create a child scope (logs: "Creating child scope from parent container")
    let request_scope = container.scope();

    // Override a service in child scope
    request_scope.singleton(RequestContext {
        request_id: "req-12345".into(),
    });

    // Resolve from child - uses local override (logs: "Service resolved from current scope")
    let _ctx = request_scope.get::<RequestContext>().unwrap();

    // Resolve from child - falls back to parent (logs: "Service resolved from parent scope")
    let _db_from_child = request_scope.get::<Database>().unwrap();

    // Create a ScopedContainer (logs: "Creating ScopedContainer from parent Container")
    let scoped = ScopedContainer::from_parent(&container);
    scoped.singleton(RequestContext {
        request_id: "scoped-req-67890".into(),
    });

    // Resolve from scoped container
    let _scoped_ctx = scoped.get::<RequestContext>().unwrap();

    // Lock the container (logs: "Container locked")
    container.lock();

    // Clear the request scope (logs: "Container cleared")
    request_scope.clear();

    println!("\n=== Demo Complete ===");
    println!("Check the log output above to see structured logging in action!");
    println!("\nTip: Use --features logging-json for production (JSON output)");
    println!("     Use --features logging-pretty for development (colorful output)");
}

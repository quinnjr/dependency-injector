//! Example demonstrating hierarchical scopes end-to-end
//!
//! Run with:
//!   cargo run --example scopes

use dependency_injector::{Container, ScopedContainer};

// App-wide services live in the root container.
#[derive(Clone)]
struct AppConfig {
    name: String,
}

#[derive(Clone)]
struct Database {
    url: String,
}

// Request/session services live in child scopes.
#[derive(Clone)]
struct RequestContext {
    request_id: String,
}

#[derive(Clone)]
struct SessionData {
    user: String,
}

fn main() {
    println!("=== Dependency Injector Scopes Demo ===\n");

    // Root container: application-wide singletons
    let root = Container::new();
    root.singleton(AppConfig {
        name: "MyApp".into(),
    });
    root.singleton(Database {
        url: "postgres://prod-db:5432/app".into(),
    });
    let config = root.get::<AppConfig>().expect("AppConfig is registered");
    println!("Root container ready: app = {}\n", config.name);

    // --- Request scope: inherits everything from the root ---
    println!("Creating a request scope with root.scope()...");
    let request_scope = root.scope();
    request_scope.singleton(RequestContext {
        request_id: "req-123".into(),
    });

    // The scope resolves its own services plus the full parent chain
    let ctx = request_scope
        .get::<RequestContext>()
        .expect("registered in this scope");
    let db = request_scope
        .get::<Database>()
        .expect("inherited from root");
    println!("  {} handled with {}", ctx.request_id, db.url);

    // Parent invisibility: the root never sees child registrations
    assert!(!root.contains::<RequestContext>());
    println!(
        "  root sees RequestContext: {}\n",
        root.contains::<RequestContext>()
    );

    // --- Override shadowing ---
    println!("Shadowing Database in a test scope...");
    let test_scope = root.scope();
    test_scope.singleton(Database {
        url: "sqlite::memory:".into(),
    });
    let test_db = test_scope.get::<Database>().expect("local override");
    let root_db = root.get::<Database>().expect("root untouched");
    println!("  test scope resolves: {}", test_db.url);
    println!("  root still resolves: {}\n", root_db.url);

    // --- remove(): drop a local registration, parent unaffected ---
    println!("Removing the override from the test scope...");
    assert!(test_scope.remove::<Database>());
    // With the override gone, resolution falls through to the parent again
    let fallback = test_scope.get::<Database>().expect("falls back to root");
    println!("  test scope now resolves: {}", fallback.url);
    assert!(root.contains::<Database>());
    // A second remove returns false - nothing left in this scope
    assert!(!test_scope.remove::<Database>());
    println!();

    // --- clear(): wipe a scope without touching the parent ---
    println!("Clearing a scratch scope...");
    let scratch = root.scope();
    scratch.singleton(RequestContext {
        request_id: "req-999".into(),
    });
    scratch.singleton(SessionData {
        user: "alice".into(),
    });
    println!("  scratch scope has {} local services", scratch.len());
    scratch.clear();
    println!("  after clear: {} local services", scratch.len());
    assert!(!scratch.contains::<SessionData>());
    // Parent services remain reachable through the cleared scope
    assert!(scratch.contains::<AppConfig>());
    println!(
        "  AppConfig still reachable from scratch scope: {}\n",
        scratch.contains::<AppConfig>()
    );

    // --- ScopedContainer: a scope with an identity ---
    println!("Creating a ScopedContainer with an explicit Scope id...");
    let session = ScopedContainer::from_parent(&root);
    session.singleton(SessionData { user: "bob".into() });

    // Each ScopedContainer carries a unique Scope id for tracking/debugging
    let scope_id = session.scope();
    println!(
        "  session scope: {scope_id} (raw id {}, depth {})",
        scope_id.id(),
        session.depth()
    );
    let user = session.get::<SessionData>().expect("registered in session");
    let cfg = session.get::<AppConfig>().expect("inherited from root");
    println!("  session for {} in app {}", user.user, cfg.name);

    // Scopes nest: derive a child from another ScopedContainer
    let nested = ScopedContainer::from_scope(&session);
    println!(
        "  nested scope: {} (depth {})",
        nested.scope(),
        nested.depth()
    );
    assert!(nested.contains::<SessionData>());
    // Sibling scopes stay isolated from each other
    assert!(!session.contains::<RequestContext>());
    println!();

    println!("=== Demo Complete ===");
    println!("\nScopes provide:");
    println!("  - Inheritance: children resolve the full parent chain");
    println!("  - Isolation: parents and siblings never see child registrations");
    println!("  - Local mutation: remove()/clear() only affect the scope itself");
}

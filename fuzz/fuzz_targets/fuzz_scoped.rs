#![no_main]

//! Fuzz target for scoped container operations
//!
//! Tests hierarchical container relationships and parent chain resolution.

use arbitrary::Arbitrary;
use dependency_injector::{Container, ScopedContainer};
use libfuzzer_sys::fuzz_target;

/// Service types
// Arbitrary-generated payload; fields vary allocation shape and are never read.
#[allow(dead_code)]
#[derive(Clone, Debug, Arbitrary)]
struct RootService {
    id: u32,
}

// Arbitrary-generated payload; fields vary allocation shape and are never read.
#[allow(dead_code)]
#[derive(Clone, Debug, Arbitrary)]
struct ScopedService {
    scope_id: u32,
    data: Vec<u8>,
}

// Arbitrary-generated payload; fields vary allocation shape and are never read.
#[allow(dead_code)]
#[derive(Clone, Debug, Arbitrary)]
struct OverrideService {
    value: String,
}

/// Operations for scoped containers
#[derive(Debug, Arbitrary)]
enum ScopedOp {
    // Root operations
    RegisterRootService(RootService),
    RegisterOverrideInRoot(OverrideService),
    GetFromRoot,

    // Scope creation
    CreateScope,
    CreateScopedContainer,
    CreateNestedScope,

    // Scoped operations
    RegisterInScope(ScopedService),
    RegisterOverrideInScope(OverrideService),
    GetFromScope,
    GetOverrideFromScope,
    GetRootFromScope,
    ContainsInScope,

    // Cleanup
    ClearScope,
    DropScope,
}

fuzz_target!(|ops: Vec<ScopedOp>| {
    let root = Container::new();
    let mut scopes: Vec<Container> = Vec::new();
    let mut scoped_containers: Vec<ScopedContainer> = Vec::new();

    for op in ops.into_iter().take(100) { // Limit operations to prevent OOM
        match op {
            ScopedOp::RegisterRootService(svc) => {
                root.singleton(svc);
            }
            ScopedOp::RegisterOverrideInRoot(svc) => {
                root.singleton(svc);
            }
            ScopedOp::GetFromRoot => {
                let _ = root.try_get::<RootService>();
            }
            ScopedOp::CreateScope => {
                if scopes.len() < 10 { // Limit depth
                    scopes.push(root.scope());
                }
            }
            ScopedOp::CreateScopedContainer => {
                if scoped_containers.len() < 10 {
                    scoped_containers.push(ScopedContainer::from_parent(&root));
                }
            }
            ScopedOp::CreateNestedScope => {
                if let Some(parent) = scopes.last()
                    && scopes.len() < 10
                {
                    scopes.push(parent.scope());
                }
            }
            ScopedOp::RegisterInScope(svc) => {
                if let Some(scope) = scopes.last() {
                    scope.singleton(svc);
                }
            }
            ScopedOp::RegisterOverrideInScope(svc) => {
                if let Some(scope) = scopes.last() {
                    scope.singleton(svc);
                }
            }
            ScopedOp::GetFromScope => {
                if let Some(scope) = scopes.last() {
                    let _ = scope.try_get::<ScopedService>();
                }
            }
            ScopedOp::GetOverrideFromScope => {
                if let Some(scope) = scopes.last() {
                    let _ = scope.try_get::<OverrideService>();
                }
            }
            ScopedOp::GetRootFromScope => {
                if let Some(scope) = scopes.last() {
                    // Should be able to resolve root services from child scope
                    let _ = scope.try_get::<RootService>();
                }
            }
            ScopedOp::ContainsInScope => {
                if let Some(scope) = scopes.last() {
                    let _ = scope.contains::<ScopedService>();
                    let _ = scope.contains::<RootService>();
                }
            }
            ScopedOp::ClearScope => {
                if let Some(scope) = scopes.last() {
                    scope.clear();
                }
            }
            ScopedOp::DropScope => {
                scopes.pop();
            }
        }
    }

    // Verify root is still functional after scope operations
    let _ = root.try_get::<RootService>();
    let _ = root.contains::<RootService>();
});


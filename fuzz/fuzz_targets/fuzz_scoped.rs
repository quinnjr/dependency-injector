#![no_main]

//! Fuzz target for scoped container operations
//!
//! Tests hierarchical container relationships and parent chain resolution.
//!
//! The shadow state below mirrors the real scope hierarchy (each scope records
//! its parent plus the values it registered itself), so every resolution can be
//! checked against the value the *nearest* scope in the chain registered. That
//! makes the override case a real assertion: resolving from a child that has
//! its own registration must yield the child's payload, never the parent's.

use arbitrary::Arbitrary;
use dependency_injector::{Container, ScopedContainer};
use libfuzzer_sys::fuzz_target;

/// Service types
#[derive(Clone, Debug, Arbitrary)]
struct RootService {
    id: u32,
}

#[derive(Clone, Debug, Arbitrary)]
struct ScopedService {
    scope_id: u32,
    data: Vec<u8>,
}

#[derive(Clone, Debug, Arbitrary)]
struct OverrideService {
    value: String,
}

/// Shadow state for one live scope in `scopes`, mirroring what that scope (and
/// only that scope) has registered locally.
struct ScopeState {
    /// Index into `scopes` of this scope's parent, or `None` when its parent is
    /// the root container. Scopes are only ever pushed with a parent that
    /// already exists and only ever popped from the end, so a parent index is
    /// always smaller than the child's own index and stays valid.
    parent: Option<usize>,
    scoped: Option<ScopedService>,
    over: Option<OverrideService>,
}

/// The `ScopedService` a resolution from `start` must yield: the one registered
/// by the nearest scope in the chain. The root never holds a `ScopedService`.
fn nearest_scoped(states: &[ScopeState], start: usize) -> Option<&ScopedService> {
    let mut idx = start;
    loop {
        if let Some(svc) = states[idx].scoped.as_ref() {
            return Some(svc);
        }
        // No parent means the chain ends at the root, which never holds one.
        idx = states[idx].parent?;
    }
}

/// The `OverrideService` a resolution from `start` must yield: the nearest
/// registration in the chain, falling back to the root's registration.
fn nearest_override<'a>(
    states: &'a [ScopeState],
    start: usize,
    root: Option<&'a OverrideService>,
) -> Option<&'a OverrideService> {
    let mut idx = start;
    loop {
        if let Some(svc) = states[idx].over.as_ref() {
            return Some(svc);
        }
        match states[idx].parent {
            Some(parent) => idx = parent,
            None => return root,
        }
    }
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

    // Shadow state: `states[i]` describes `scopes[i]`; the two vectors are
    // always pushed and popped in lockstep.
    let mut states: Vec<ScopeState> = Vec::new();
    let mut root_service: Option<RootService> = None;
    let mut root_override: Option<OverrideService> = None;

    for op in ops.into_iter().take(100) { // Limit operations to prevent OOM
        match op {
            ScopedOp::RegisterRootService(svc) => {
                root_service = Some(svc.clone());
                root.singleton(svc);
            }
            ScopedOp::RegisterOverrideInRoot(svc) => {
                root_override = Some(svc.clone());
                root.singleton(svc);
            }
            ScopedOp::GetFromRoot => {
                let actual = root.try_get::<RootService>();
                assert_eq!(
                    actual.is_some(),
                    root_service.is_some(),
                    "root resolves RootService iff it is registered"
                );
                if let (Some(actual), Some(expected)) = (&actual, &root_service) {
                    assert_eq!(actual.id, expected.id, "RootService::id must round-trip");
                }
            }
            ScopedOp::CreateScope => {
                if scopes.len() < 10 { // Limit depth
                    scopes.push(root.scope());
                    states.push(ScopeState {
                        parent: None,
                        scoped: None,
                        over: None,
                    });
                }
            }
            ScopedOp::CreateScopedContainer => {
                if scoped_containers.len() < 10 {
                    scoped_containers.push(ScopedContainer::from_parent(&root));
                }
            }
            ScopedOp::CreateNestedScope => {
                if scopes.len() < 10
                    && let Some(parent) = scopes.last()
                {
                    let child = parent.scope();
                    let parent_idx = scopes.len() - 1;
                    scopes.push(child);
                    states.push(ScopeState {
                        parent: Some(parent_idx),
                        scoped: None,
                        over: None,
                    });
                }
            }
            ScopedOp::RegisterInScope(svc) => {
                if let Some(scope) = scopes.last() {
                    let state = states.last_mut().expect("shadow state stays in lockstep");
                    state.scoped = Some(svc.clone());
                    scope.singleton(svc);
                }
            }
            ScopedOp::RegisterOverrideInScope(svc) => {
                if let Some(scope) = scopes.last() {
                    let state = states.last_mut().expect("shadow state stays in lockstep");
                    state.over = Some(svc.clone());
                    scope.singleton(svc);
                }
            }
            ScopedOp::GetFromScope => {
                if let Some(scope) = scopes.last() {
                    let expected = nearest_scoped(&states, scopes.len() - 1);
                    let actual = scope.try_get::<ScopedService>();
                    assert_eq!(
                        actual.is_some(),
                        expected.is_some(),
                        "ScopedService resolves iff some scope in the chain registered it"
                    );
                    if let (Some(actual), Some(expected)) = (&actual, expected) {
                        assert_eq!(
                            actual.scope_id, expected.scope_id,
                            "ScopedService::scope_id must come from the nearest scope"
                        );
                        assert_eq!(
                            actual.data, expected.data,
                            "ScopedService::data must come from the nearest scope"
                        );
                    }
                }
            }
            ScopedOp::GetOverrideFromScope => {
                if let Some(scope) = scopes.last() {
                    let expected =
                        nearest_override(&states, scopes.len() - 1, root_override.as_ref());
                    let actual = scope.try_get::<OverrideService>();
                    assert_eq!(
                        actual.is_some(),
                        expected.is_some(),
                        "OverrideService resolves iff the chain registered it"
                    );
                    if let (Some(actual), Some(expected)) = (&actual, expected) {
                        // The nearest registration wins: a child override must
                        // shadow the parent's (and the root's) value.
                        assert_eq!(
                            actual.value, expected.value,
                            "OverrideService::value must come from the nearest scope"
                        );
                    }
                }
            }
            ScopedOp::GetRootFromScope => {
                if let Some(scope) = scopes.last() {
                    // Should be able to resolve root services from child scope.
                    // Only the root ever registers RootService, so the whole
                    // chain must agree with the root's shadow state.
                    let actual = scope.try_get::<RootService>();
                    assert_eq!(
                        actual.is_some(),
                        root_service.is_some(),
                        "RootService must be visible through the parent chain"
                    );
                    if let (Some(actual), Some(expected)) = (&actual, &root_service) {
                        assert_eq!(
                            actual.id, expected.id,
                            "RootService::id must round-trip through the parent chain"
                        );
                    }
                }
            }
            ScopedOp::ContainsInScope => {
                if let Some(scope) = scopes.last() {
                    // `contains` walks the same chain as resolution.
                    assert_eq!(
                        scope.contains::<ScopedService>(),
                        nearest_scoped(&states, scopes.len() - 1).is_some()
                    );
                    assert_eq!(scope.contains::<RootService>(), root_service.is_some());
                }
            }
            ScopedOp::ClearScope => {
                if let Some(scope) = scopes.last() {
                    // `clear()` only affects this scope's own storage; parent
                    // registrations stay visible through the chain.
                    scope.clear();
                    let state = states.last_mut().expect("shadow state stays in lockstep");
                    state.scoped = None;
                    state.over = None;
                }
            }
            ScopedOp::DropScope => {
                scopes.pop();
                states.pop();
            }
        }
    }

    // Verify root is still functional after scope operations
    let actual = root.try_get::<RootService>();
    assert_eq!(
        actual.is_some(),
        root_service.is_some(),
        "root must still resolve RootService after scope churn"
    );
    if let (Some(actual), Some(expected)) = (&actual, &root_service) {
        assert_eq!(
            actual.id, expected.id,
            "RootService::id must survive scope churn"
        );
    }
    assert_eq!(root.contains::<RootService>(), root_service.is_some());
});

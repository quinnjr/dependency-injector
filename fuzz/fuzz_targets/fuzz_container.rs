#![no_main]

//! Fuzz target for basic container operations
//!
//! Tests registration and resolution with various data patterns.

use arbitrary::{Arbitrary, Unstructured};
use dependency_injector::Container;
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;

/// Service types for fuzzing
#[derive(Clone, Debug, Arbitrary)]
struct SmallService {
    id: u32,
    name: String,
}

#[derive(Clone, Debug, Arbitrary)]
struct MediumService {
    id: u64,
    data: Vec<u8>,
    config: ServiceConfig,
}

#[derive(Clone, Debug, Arbitrary)]
struct ServiceConfig {
    enabled: bool,
    timeout_ms: u32,
    retries: u8,
    tags: Vec<String>,
}

#[derive(Clone, Debug, Arbitrary)]
struct LargeService {
    id: u128,
    payload: Vec<u8>,
    metadata: Vec<(String, String)>,
}

/// Operations to perform on the container
#[derive(Debug, Arbitrary)]
enum ContainerOp {
    RegisterSmall(SmallService),
    RegisterMedium(MediumService),
    RegisterLarge(LargeService),
    RegisterLazySmall,
    RegisterTransientSmall,
    GetSmall,
    GetMedium,
    GetLarge,
    TryGetSmall,
    TryGetMedium,
    ContainsSmall,
    ContainsMedium,
    ContainsLarge,
    Clear,
    GetLen,
    IsEmpty,
}

fuzz_target!(|ops: Vec<ContainerOp>| {
    let container = Container::new();

    // No cache hygiene needed: hot-cache entries are generation-stamped and
    // storage generations are globally unique, so entries left behind by a
    // previous iteration's container can never hit on this one. Not clearing
    // here also lets the fuzzer exercise that invalidation mechanism.

    // Shadow registration state. On this root container (no parent scopes),
    // registration via singleton/lazy/transient always succeeds and overwrites
    // any previous registration for the same type, so each boolean exactly
    // tracks whether resolution of that type must succeed.
    let mut has_small = false;
    let mut has_medium = false;
    let mut has_large = false;

    for op in ops {
        match op {
            ContainerOp::RegisterSmall(svc) => {
                container.singleton(svc);
                has_small = true;
            }
            ContainerOp::RegisterMedium(svc) => {
                container.singleton(svc);
                has_medium = true;
            }
            ContainerOp::RegisterLarge(svc) => {
                container.singleton(svc);
                has_large = true;
            }
            ContainerOp::RegisterLazySmall => {
                container.lazy(|| SmallService {
                    id: 42,
                    name: "lazy".into(),
                });
                has_small = true;
            }
            ContainerOp::RegisterTransientSmall => {
                container.transient(|| SmallService {
                    id: 0,
                    name: "transient".into(),
                });
                has_small = true;
            }
            ContainerOp::GetSmall => {
                // On a root container, resolution succeeds iff registered.
                let result = container.get::<SmallService>();
                assert_eq!(result.is_ok(), has_small);
            }
            ContainerOp::GetMedium => {
                let result = container.get::<MediumService>();
                assert_eq!(result.is_ok(), has_medium);
            }
            ContainerOp::GetLarge => {
                let result = container.get::<LargeService>();
                assert_eq!(result.is_ok(), has_large);
            }
            ContainerOp::TryGetSmall => {
                assert_eq!(container.try_get::<SmallService>().is_some(), has_small);
            }
            ContainerOp::TryGetMedium => {
                assert_eq!(container.try_get::<MediumService>().is_some(), has_medium);
            }
            ContainerOp::ContainsSmall => {
                // `contains` reads storage directly (no hot cache), so it must
                // agree with the shadow registration state at all times.
                assert_eq!(container.contains::<SmallService>(), has_small);
            }
            ContainerOp::ContainsMedium => {
                assert_eq!(container.contains::<MediumService>(), has_medium);
            }
            ContainerOp::ContainsLarge => {
                assert_eq!(container.contains::<LargeService>(), has_large);
            }
            ContainerOp::Clear => {
                // `clear()` invalidates the hot cache automatically (the
                // storage stamps a fresh generation on every mutation), so the
                // Ok/Err assertions below also verify that invalidation works.
                container.clear();
                has_small = false;
                has_medium = false;
                has_large = false;
            }
            ContainerOp::GetLen => {
                // The three fuzzed types are the only ones ever registered, and
                // re-registering a type overwrites rather than adds, so `len()`
                // must equal the number of currently registered types.
                let expected = usize::from(has_small)
                    + usize::from(has_medium)
                    + usize::from(has_large);
                assert_eq!(container.len(), expected);
            }
            ContainerOp::IsEmpty => {
                assert_eq!(
                    container.is_empty(),
                    !(has_small || has_medium || has_large)
                );
            }
        }
    }
});


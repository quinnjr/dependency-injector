#![no_main]

//! Fuzz target for basic container operations
//!
//! Tests registration and resolution with various data patterns.
//!
//! Every payload field is fuzzer-controlled (it varies allocation shape and
//! payload size) *and* read back after resolution: the shadow state below
//! records the value that was registered, and every successful resolution is
//! checked field-by-field against it. That turns "the container did not crash"
//! into "the container round-tripped exactly what was registered".

use arbitrary::Arbitrary;
use dependency_injector::Container;
use libfuzzer_sys::fuzz_target;

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

/// A resolved `SmallService` must carry exactly the registered payload.
fn assert_small_eq(actual: &SmallService, expected: &SmallService) {
    assert_eq!(actual.id, expected.id, "SmallService::id must round-trip");
    assert_eq!(
        actual.name, expected.name,
        "SmallService::name must round-trip"
    );
}

/// A resolved `ServiceConfig` must carry exactly the registered payload.
fn assert_config_eq(actual: &ServiceConfig, expected: &ServiceConfig) {
    assert_eq!(
        actual.enabled, expected.enabled,
        "ServiceConfig::enabled must round-trip"
    );
    assert_eq!(
        actual.timeout_ms, expected.timeout_ms,
        "ServiceConfig::timeout_ms must round-trip"
    );
    assert_eq!(
        actual.retries, expected.retries,
        "ServiceConfig::retries must round-trip"
    );
    assert_eq!(
        actual.tags, expected.tags,
        "ServiceConfig::tags must round-trip"
    );
}

/// A resolved `MediumService` must carry exactly the registered payload,
/// including every field of the nested [`ServiceConfig`].
fn assert_medium_eq(actual: &MediumService, expected: &MediumService) {
    assert_eq!(actual.id, expected.id, "MediumService::id must round-trip");
    assert_eq!(
        actual.data, expected.data,
        "MediumService::data must round-trip"
    );
    assert_config_eq(&actual.config, &expected.config);
}

/// A resolved `LargeService` must carry exactly the registered payload.
fn assert_large_eq(actual: &LargeService, expected: &LargeService) {
    assert_eq!(actual.id, expected.id, "LargeService::id must round-trip");
    assert_eq!(
        actual.payload, expected.payload,
        "LargeService::payload must round-trip"
    );
    assert_eq!(
        actual.metadata, expected.metadata,
        "LargeService::metadata must round-trip"
    );
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
    // any previous registration for the same type, so each slot exactly tracks
    // whether resolution of that type must succeed *and* which value it must
    // yield. The lazy/transient factories used here are deterministic (they
    // return a constant), so the expected value is known for those too.
    let mut small: Option<SmallService> = None;
    let mut medium: Option<MediumService> = None;
    let mut large: Option<LargeService> = None;

    for op in ops {
        match op {
            ContainerOp::RegisterSmall(svc) => {
                small = Some(svc.clone());
                container.singleton(svc);
            }
            ContainerOp::RegisterMedium(svc) => {
                medium = Some(svc.clone());
                container.singleton(svc);
            }
            ContainerOp::RegisterLarge(svc) => {
                large = Some(svc.clone());
                container.singleton(svc);
            }
            ContainerOp::RegisterLazySmall => {
                container.lazy(|| SmallService {
                    id: 42,
                    name: "lazy".into(),
                });
                // Deterministic factory: the lazy singleton can only ever
                // produce this exact value, whenever it is first resolved.
                small = Some(SmallService {
                    id: 42,
                    name: "lazy".into(),
                });
            }
            ContainerOp::RegisterTransientSmall => {
                container.transient(|| SmallService {
                    id: 0,
                    name: "transient".into(),
                });
                // Deterministic factory: every transient instance is a fresh
                // allocation but always carries these same field values.
                small = Some(SmallService {
                    id: 0,
                    name: "transient".into(),
                });
            }
            ContainerOp::GetSmall => {
                // On a root container, resolution succeeds iff registered, and
                // the resolved value must match what was registered.
                let result = container.get::<SmallService>();
                assert_eq!(result.is_ok(), small.is_some());
                if let (Ok(svc), Some(expected)) = (&result, &small) {
                    assert_small_eq(svc, expected);
                }
            }
            ContainerOp::GetMedium => {
                let result = container.get::<MediumService>();
                assert_eq!(result.is_ok(), medium.is_some());
                if let (Ok(svc), Some(expected)) = (&result, &medium) {
                    assert_medium_eq(svc, expected);
                }
            }
            ContainerOp::GetLarge => {
                let result = container.get::<LargeService>();
                assert_eq!(result.is_ok(), large.is_some());
                if let (Ok(svc), Some(expected)) = (&result, &large) {
                    assert_large_eq(svc, expected);
                }
            }
            ContainerOp::TryGetSmall => {
                let result = container.try_get::<SmallService>();
                assert_eq!(result.is_some(), small.is_some());
                if let (Some(svc), Some(expected)) = (&result, &small) {
                    assert_small_eq(svc, expected);
                }
            }
            ContainerOp::TryGetMedium => {
                let result = container.try_get::<MediumService>();
                assert_eq!(result.is_some(), medium.is_some());
                if let (Some(svc), Some(expected)) = (&result, &medium) {
                    assert_medium_eq(svc, expected);
                }
            }
            ContainerOp::ContainsSmall => {
                // `contains` reads storage directly (no hot cache), so it must
                // agree with the shadow registration state at all times.
                assert_eq!(container.contains::<SmallService>(), small.is_some());
            }
            ContainerOp::ContainsMedium => {
                assert_eq!(container.contains::<MediumService>(), medium.is_some());
            }
            ContainerOp::ContainsLarge => {
                assert_eq!(container.contains::<LargeService>(), large.is_some());
            }
            ContainerOp::Clear => {
                // `clear()` invalidates the hot cache automatically (the
                // storage stamps a fresh generation on every mutation), so the
                // Ok/Err assertions below also verify that invalidation works.
                container.clear();
                small = None;
                medium = None;
                large = None;
            }
            ContainerOp::GetLen => {
                // The three fuzzed types are the only ones ever registered, and
                // re-registering a type overwrites rather than adds, so `len()`
                // must equal the number of currently registered types.
                let expected = usize::from(small.is_some())
                    + usize::from(medium.is_some())
                    + usize::from(large.is_some());
                assert_eq!(container.len(), expected);
            }
            ContainerOp::IsEmpty => {
                assert_eq!(
                    container.is_empty(),
                    !(small.is_some() || medium.is_some() || large.is_some())
                );
            }
        }
    }
});

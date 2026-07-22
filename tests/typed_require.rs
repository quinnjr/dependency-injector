//! Compile-pass tests for the `#[derive(TypedRequire)]` macro.
//!
//! Run with:
//!   cargo test --test typed_require --features derive
#![cfg(feature = "derive")]

use dependency_injector::TypedRequire;
use dependency_injector::typed::{Reg, Require, TypedBuilder, TypedContainer};
use std::sync::Arc;

#[derive(Clone)]
struct Database;

#[derive(Clone)]
struct Cache;

#[allow(dead_code)]
#[derive(Clone, TypedRequire)]
struct UserService {
    #[dep]
    db: Arc<Database>,
    #[dep]
    cache: Arc<Cache>,
}

#[allow(dead_code)]
#[derive(Clone, TypedRequire)]
struct Standalone {
    // Non-dep fields are excluded from the dependency list.
    counter: u64,
}

#[allow(dead_code)]
#[derive(Clone, TypedRequire)]
struct WithOptional {
    #[dep]
    db: Arc<Database>,
    // Optional deps are excluded from the required-dependency list.
    #[dep(optional)]
    cache: Option<Arc<Cache>>,
}

/// Compile-time check that `S::Dependencies` is exactly `D`.
fn assert_dependencies<S: Require<Dependencies = D>, D>() {}

/// Compile-time check that `S::Dependencies` matches a built registry type.
fn assert_matches_registry<S: Require<Dependencies = R>, R>(_: &TypedContainer<R>) {}

#[test]
fn generates_reg_chain_for_dep_fields() {
    // Fields are listed head-first: db, then cache, terminated by ().
    assert_dependencies::<UserService, Reg<Database, Reg<Cache, ()>>>();
}

#[test]
fn generates_unit_for_no_dep_fields() {
    assert_dependencies::<Standalone, ()>();
}

#[test]
fn excludes_optional_deps_from_required_list() {
    assert_dependencies::<WithOptional, Reg<Database, ()>>();
}

#[test]
fn dependencies_match_typed_builder_registry() {
    // Registering Cache first, then Database, yields the registry type
    // Reg<Database, Reg<Cache, ()>> - exactly the list UserService declares.
    let container = TypedBuilder::new()
        .singleton(Cache)
        .singleton(Database)
        .build();

    assert_matches_registry::<UserService, _>(&container);

    let _ = container.get::<Database>();
    let _ = container.get::<Cache>();
}

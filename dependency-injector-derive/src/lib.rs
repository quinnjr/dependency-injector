//! Derive macros for dependency-injector
//!
//! This crate provides derive macros for automatic dependency injection:
//!
//! - `#[derive(Inject)]` - Generate `from_container()` for runtime DI
//! - `#[derive(Service)]` - Generate `Service` trait impl for compile-time verified DI
//! - `#[derive(TypedRequire)]` - Generate `Require` trait impl declaring type-level dependencies
//!
//! All three derives accept `#[inject]` and `#[dep]` (and their `(optional)`
//! forms) as exact aliases for marking dependency fields. By convention,
//! `Inject` uses `#[inject]` while `Service` and `TypedRequire` use `#[dep]`.
//!
//! # Inject Example
//!
//! ```rust,ignore
//! use dependency_injector::{Container, Inject};
//! use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct Database {
//!     url: String,
//! }
//!
//! #[derive(Clone)]
//! struct Cache {
//!     size: usize,
//! }
//!
//! #[derive(Inject)]
//! struct UserService {
//!     #[inject]
//!     db: Arc<Database>,
//!     #[inject]
//!     cache: Arc<Cache>,
//!     // Non-injected fields use Default
//!     request_count: u64,
//! }
//!
//! let container = Container::new();
//! container.singleton(Database { url: "postgres://localhost".into() });
//! container.singleton(Cache { size: 1024 });
//!
//! let service = UserService::from_container(&container).unwrap();
//! ```
//!
//! # Service Example (Compile-Time Safety)
//!
//! ```rust,ignore
//! use dependency_injector::{verified::{Service, ServiceProvider}, Container};
//! use dependency_injector_derive::Service;
//! use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct Config { debug: bool }
//!
//! #[derive(Clone, Service)]
//! struct Database {
//!     #[dep]
//!     config: Arc<Config>,
//! }
//!
//! let container = Container::new();
//! container.singleton(Config { debug: true });
//! container.provide::<Database>();  // Deps verified at compile time!
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Fields, Type, parse_macro_input};

/// Derive macro for automatic dependency injection.
///
/// Generates a `from_container()` method that resolves dependencies
/// from a `Container` instance.
///
/// # Attributes
///
/// - `#[inject]` - Mark a field for injection. The field type must be `Arc<T>`.
/// - `#[inject(optional)]` - Mark a field as optional injection. Uses `Option<Arc<T>>`.
///
/// `#[dep]` and `#[dep(optional)]` are accepted as exact aliases of
/// `#[inject]` and `#[inject(optional)]` respectively, so the same field
/// annotations work across every derive in this crate. `#[inject]` is the
/// conventional spelling for this macro.
///
/// # Generated Methods
///
/// - `from_container(container: &Container) -> Result<Self, DiError>` - Creates an instance
///   by resolving all `#[inject]` fields from the container.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Inject)]
/// struct MyService {
///     #[inject]
///     db: Arc<Database>,
///     #[inject(optional)]
///     cache: Option<Arc<Cache>>,
///     // Fields without #[inject] use Default::default()
///     counter: u64,
/// }
/// ```
#[proc_macro_derive(Inject, attributes(inject, dep))]
pub fn derive_inject(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Only support structs with named fields
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "Inject can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Inject can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Parse fields and generate initialization code
    let mut field_inits = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;

        let inject_attr = find_dep_attr(&field.attrs);

        match inject_attr {
            Some(DepAttr::Required) => {
                // Extract inner type from Arc<T>
                if let Some(inner_type) = extract_arc_inner_type(field_type) {
                    field_inits.push(quote! {
                        #field_name: container.get::<#inner_type>()?
                    });
                } else {
                    return syn::Error::new_spanned(
                        field_type,
                        "Fields marked with #[inject]/#[dep] must have type Arc<T>",
                    )
                    .to_compile_error()
                    .into();
                }
            }
            Some(DepAttr::Optional) => {
                // Extract inner type from Option<Arc<T>>
                if let Some(inner_type) = extract_option_arc_inner_type(field_type) {
                    field_inits.push(quote! {
                        #field_name: container.try_get::<#inner_type>()
                    });
                } else {
                    return syn::Error::new_spanned(
                        field_type,
                        "Fields marked with #[inject(optional)]/#[dep(optional)] must have type Option<Arc<T>>",
                    )
                    .to_compile_error()
                    .into();
                }
            }
            None => {
                // Non-injected field - use Default
                field_inits.push(quote! {
                    #field_name: ::std::default::Default::default()
                });
            }
        }
    }

    // Generate the implementation
    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Create an instance by resolving dependencies from a container.
            ///
            /// All fields marked with `#[inject]` (or its alias `#[dep]`) will be
            /// resolved from the container. Unmarked fields use `Default::default()`.
            pub fn from_container(
                container: &::dependency_injector::Container
            ) -> ::dependency_injector::Result<Self> {
                Ok(Self {
                    #(#field_inits),*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

/// Kinds of dependency-marker attributes shared by every derive in this crate.
enum DepAttr {
    Required,
    Optional,
}

/// Find and parse a dependency-marker attribute on a field.
///
/// `#[inject]` and `#[dep]` are exact aliases, as are `#[inject(optional)]`
/// and `#[dep(optional)]`. Every derive in this crate accepts both spellings
/// and treats them identically.
fn find_dep_attr(attrs: &[Attribute]) -> Option<DepAttr> {
    for attr in attrs {
        if attr.path().is_ident("inject") || attr.path().is_ident("dep") {
            // Bare `#[inject]` / `#[dep]` means required
            if attr.meta.require_path_only().is_ok() {
                return Some(DepAttr::Required);
            }

            // Parse `#[inject(optional)]` / `#[dep(optional)]`
            if let Ok(nested) = attr.parse_args::<syn::Ident>() {
                if nested == "optional" {
                    return Some(DepAttr::Optional);
                }
            }

            // Default to required
            return Some(DepAttr::Required);
        }
    }
    None
}

/// Extract T from Arc<T>
fn extract_arc_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "Arc" {
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner);
                }
            }
        }
    }
    None
}

/// Extract T from Option<Arc<T>>
fn extract_option_arc_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "Option" {
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return extract_arc_inner_type(inner);
                }
            }
        }
    }
    None
}

// =============================================================================
// Service Derive Macro
// =============================================================================

/// Derive macro for the `Service` trait.
///
/// Generates a `Service` implementation with compile-time dependency declaration.
/// This enables type-safe dependency injection with verification at compile time.
///
/// # Attributes
///
/// - `#[dep]` - Mark a field as a required dependency. Must be `Arc<T>`.
/// - `#[dep(optional)]` - Mark a field as optional. Must be `Option<Arc<T>>`.
///
/// `#[inject]` and `#[inject(optional)]` are accepted as exact aliases of
/// `#[dep]` and `#[dep(optional)]` respectively, so the same field
/// annotations work across every derive in this crate. `#[dep]` is the
/// conventional spelling for this macro.
///
/// Fields without a dependency marker use `Default::default()`.
///
/// # Generated Code
///
/// The macro generates:
/// - `type Dependencies` - Tuple of all dependency types
/// - `fn create(deps) -> Self` - Creates the service from dependencies
///
/// # Example
///
/// ```rust,ignore
/// use dependency_injector_derive::Service;
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct Config;
///
/// #[derive(Clone)]
/// struct Database;
///
/// #[derive(Clone, Service)]
/// struct UserService {
///     #[dep]
///     config: Arc<Config>,
///     #[dep]
///     db: Arc<Database>,
///     // Non-dep fields use Default
///     request_count: u64,
/// }
///
/// // Generated implementation:
/// // impl Service for UserService {
/// //     type Dependencies = (Arc<Config>, Arc<Database>);
/// //     fn create((config, db): Self::Dependencies) -> Self {
/// //         Self { config, db, request_count: Default::default() }
/// //     }
/// // }
/// ```
#[proc_macro_derive(Service, attributes(dep, inject))]
pub fn derive_service(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Only support structs with named fields
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "Service can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Service can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Collect dependency types and field initializers
    let mut dep_types: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut dep_names: Vec<syn::Ident> = Vec::new();
    let mut field_inits: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut dep_index = 0usize;

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;

        let dep_attr = find_dep_attr(&field.attrs);

        match dep_attr {
            Some(DepAttr::Required) => {
                // Field is Arc<T>, extract T for dependency type
                if extract_arc_inner_type(field_type).is_some() {
                    let dep_name =
                        syn::Ident::new(&format!("__dep_{}", dep_index), field_name.span());
                    dep_types.push(quote! { #field_type });
                    dep_names.push(dep_name.clone());
                    field_inits.push(quote! { #field_name: #dep_name });
                    dep_index += 1;
                } else {
                    return syn::Error::new_spanned(
                        field_type,
                        "Fields marked with #[dep]/#[inject] must have type Arc<T>",
                    )
                    .to_compile_error()
                    .into();
                }
            }
            Some(DepAttr::Optional) => {
                // Field is Option<Arc<T>>
                if extract_option_arc_inner_type(field_type).is_some() {
                    let dep_name =
                        syn::Ident::new(&format!("__dep_{}", dep_index), field_name.span());
                    dep_types.push(quote! { #field_type });
                    dep_names.push(dep_name.clone());
                    field_inits.push(quote! { #field_name: #dep_name });
                    dep_index += 1;
                } else {
                    return syn::Error::new_spanned(
                        field_type,
                        "Fields marked with #[dep(optional)]/#[inject(optional)] must have type Option<Arc<T>>",
                    )
                    .to_compile_error()
                    .into();
                }
            }
            None => {
                // Non-dependency field - use Default
                field_inits.push(quote! {
                    #field_name: ::std::default::Default::default()
                });
            }
        }
    }

    // Generate the Dependencies type and create function
    let (deps_type, deps_pattern) = match dep_types.len() {
        0 => (quote! { () }, quote! { _ }),
        1 => {
            let ty = &dep_types[0];
            let name = &dep_names[0];
            (quote! { #ty }, quote! { #name })
        }
        _ => {
            let types = &dep_types;
            let names = &dep_names;
            (quote! { (#(#types),*) }, quote! { (#(#names),*) })
        }
    };

    // Generate the implementation
    let expanded = quote! {
        impl #impl_generics ::dependency_injector::verified::Service for #name #ty_generics #where_clause {
            type Dependencies = #deps_type;

            fn create(#deps_pattern: Self::Dependencies) -> Self {
                Self {
                    #(#field_inits),*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

// =============================================================================
// TypedRequire Derive Macro
// =============================================================================

/// Derive macro for the `Require` trait (type-level dependencies).
///
/// Generates a `dependency_injector::typed::Require` implementation that
/// declares dependencies at the type level as a `Reg` chain terminated by
/// `()`, mirroring the registry type built by `TypedBuilder`.
///
/// # Attributes
///
/// - `#[dep]` - Mark a field as a required dependency. Must be `Arc<T>`.
///
/// `#[inject]` is accepted as an exact alias of `#[dep]` (and
/// `#[inject(optional)]` of `#[dep(optional)]`), so the same field
/// annotations work across every derive in this crate. `#[dep]` is the
/// conventional spelling for this macro.
///
/// Fields without a dependency marker, and fields marked
/// `#[dep(optional)]`/`#[inject(optional)]`, are not included in the
/// dependency list (optional dependencies need not be present in a
/// registry).
///
/// # Example
///
/// ```rust,ignore
/// use dependency_injector::TypedRequire;
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct Database;
///
/// #[derive(Clone)]
/// struct Cache;
///
/// #[derive(Clone, TypedRequire)]
/// struct UserService {
///     #[dep]
///     db: Arc<Database>,
///     #[dep]
///     cache: Arc<Cache>,
/// }
///
/// // Generated:
/// // impl dependency_injector::typed::Require for UserService {
/// //     type Dependencies = Reg<Database, Reg<Cache, ()>>;
/// // }
/// ```
#[proc_macro_derive(TypedRequire, attributes(dep, inject))]
pub fn derive_typed_require(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Only support structs with named fields
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "TypedRequire can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "TypedRequire can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Collect dependency types (only Arc<T> fields marked #[dep]/#[inject])
    let mut inner_types: Vec<&Type> = Vec::new();

    for field in fields {
        let field_type = &field.ty;
        let dep_attr = find_dep_attr(&field.attrs);

        if let Some(DepAttr::Required) = dep_attr {
            if let Some(inner) = extract_arc_inner_type(field_type) {
                inner_types.push(inner);
            } else {
                return syn::Error::new_spanned(
                    field_type,
                    "Fields marked with #[dep]/#[inject] must have type Arc<T>",
                )
                .to_compile_error()
                .into();
            }
        }
    }

    // Build the type-level list: Reg<T1, Reg<T2, ... Reg<Tn, ()>>>
    let deps_type = inner_types.iter().rev().fold(quote! { () }, |acc, ty| {
        quote! { ::dependency_injector::typed::Reg<#ty, #acc> }
    });

    // Generate the implementation
    let expanded = quote! {
        impl #impl_generics ::dependency_injector::typed::Require for #name #ty_generics #where_clause {
            type Dependencies = #deps_type;
        }
    };

    TokenStream::from(expanded)
}

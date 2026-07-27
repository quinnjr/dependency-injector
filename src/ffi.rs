//! FFI (Foreign Function Interface) bindings for dependency-injector.
//!
//! This module provides a C-compatible API for using the dependency injection
//! container from other languages like Go, Python, C, etc.
//!
//! # Design
//!
//! Since Rust generics cannot cross FFI boundaries, services are registered
//! and resolved by string type names. Service data is passed as raw bytes
//! that can be serialized/deserialized on the foreign language side.
//!
//! # Memory Management
//!
//! - `di_container_new()` allocates a container - must be freed with `di_container_free()`
//! - `di_service_*` functions return service handles - must be freed with `di_service_free()`
//! - `di_error_message()` returns a string - must be freed with `di_string_free()`
//!
//! # Thread Safety
//!
//! The container is thread-safe. All FFI functions can be called from multiple threads.
//! Panics are caught at the FFI boundary (never unwinding into the caller) and surfaced
//! via `di_error_message()`.

use crate::error::DiError;
use std::any::Any;
use std::collections::{HashMap, hash_map::Entry};
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

/// Opaque container handle for FFI
pub struct DiContainer {
    /// Map of type names to their registered services (as raw bytes)
    services: RwLock<HashMap<String, Arc<dyn Any + Send + Sync>>>,
    /// Lock state - when set, registration is blocked (removal is not),
    /// matching the core container's locking semantics.
    locked: AtomicBool,
}

/// Opaque service handle for FFI
pub struct DiService {
    type_name: String,
    data: Vec<u8>,
}

/// Error codes returned by FFI functions
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiErrorCode {
    /// Operation succeeded
    Ok = 0,
    /// Service not found
    NotFound = 1,
    /// Invalid argument (null pointer, invalid UTF-8, etc.)
    InvalidArgument = 2,
    /// Service already registered
    AlreadyRegistered = 3,
    /// Internal error
    InternalError = 4,
    /// Serialization/deserialization error
    SerializationError = 5,
    /// Container is locked - registration is not allowed
    Locked = 6,
}

/// Maps core [`DiError`] values to ABI error codes.
///
/// `CircularDependency`, `CreationFailed`, `ParentDropped`, and `Internal`
/// currently collapse to [`DiErrorCode::InternalError`]; dedicated codes for
/// them can be added to the ABI in a future major version.
impl From<&DiError> for DiErrorCode {
    fn from(err: &DiError) -> Self {
        match err {
            DiError::NotFound { .. } => Self::NotFound,
            DiError::AlreadyRegistered { .. } => Self::AlreadyRegistered,
            DiError::Locked => Self::Locked,
            DiError::CircularDependency { .. }
            | DiError::CreationFailed { .. }
            | DiError::ParentDropped
            | DiError::Internal(_) => Self::InternalError,
        }
    }
}

/// Result type for FFI operations
#[repr(C)]
pub struct DiResult {
    pub code: DiErrorCode,
    pub service: *mut DiService,
}

// Thread-local storage for the last error message
thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Record `msg` as the thread-local last error.
///
/// Must stay allocation-only and non-reentrant (no logging, callbacks, or
/// re-entry into FFI functions): the `Err` arm of [`ffi_guard`] calls this
/// OUTSIDE `catch_unwind`, so a panic raised here would unwind straight
/// across the FFI boundary.
fn set_last_error(msg: impl Into<String>) {
    // Replace control characters (incl. newlines) with spaces: type names
    // are caller-supplied and error strings flow into host application
    // logs verbatim, so this blocks log-forging via crafted names.
    let sanitized: String = msg
        .into()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = Some(sanitized);
    });
}

/// Run `f`, catching any panic so it cannot unwind across the FFI boundary.
///
/// On panic, records "internal panic: <message>" as the thread-local last
/// error (via [`set_last_error`]) and returns `default` instead. Raw-pointer
/// arguments captured by `f` are not `UnwindSafe`, hence [`AssertUnwindSafe`];
/// the pointers themselves are plain values that cannot be observed in a
/// broken state, so this is sound.
fn ffi_guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                format!("internal panic: {s}")
            } else if let Some(s) = payload.downcast_ref::<String>() {
                format!("internal panic: {s}")
            } else {
                String::from("internal panic")
            };
            set_last_error(msg);
            default
        }
    }
}

/// Test-only entry point that routes an explicit `&str` panic through
/// [`ffi_guard`]: recovering from lock poisoning removed the last internal
/// panic source, so this keeps the catch-unwind path directly exercised.
#[cfg(test)]
fn panicking_entry_str() -> DiErrorCode {
    ffi_guard(DiErrorCode::InternalError, || panic!("test panic: boom"))
}

/// Companion to [`panicking_entry_str`] covering the `String` payload branch
/// of [`ffi_guard`]'s downcast (format arguments produce a `String` payload).
#[cfg(test)]
fn panicking_entry_string() -> DiErrorCode {
    ffi_guard(DiErrorCode::InternalError, || {
        let detail = String::from("boom-string");
        panic!("test panic: {detail}")
    })
}

// ============================================================================
// Container Lifecycle
// ============================================================================

/// Create a new dependency injection container.
///
/// # Returns
/// A pointer to the new container, or NULL on failure.
///
/// # Safety
/// The returned pointer must be freed with `di_container_free()`.
#[unsafe(no_mangle)]
pub extern "C" fn di_container_new() -> *mut DiContainer {
    ffi_guard(ptr::null_mut(), || {
        let container = Box::new(DiContainer {
            services: RwLock::new(HashMap::new()),
            locked: AtomicBool::new(false),
        });
        Box::into_raw(container)
    })
}

/// Free a container and all its resources.
///
/// # Safety
/// - `container` must be a valid pointer returned by `di_container_new()`
/// - After calling this function, the pointer is invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_container_free(container: *mut DiContainer) {
    ffi_guard((), || {
        if !container.is_null() {
            // SAFETY: Caller guarantees container is valid
            drop(unsafe { Box::from_raw(container) });
        }
    });
}

/// Create a child scope from a container.
///
/// Inheritance is a snapshot taken at creation time: the child receives a
/// copy of the parent's services as they exist when the scope is created.
/// Services registered in the parent afterwards are not visible to existing
/// child scopes. The child starts unlocked regardless of the parent's lock
/// state, matching the core container's scoping semantics.
///
/// # Returns
/// A pointer to the new scoped container, or NULL on failure.
///
/// # Safety
/// - `container` must be a valid container pointer
/// - The returned pointer must be freed with `di_container_free()`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_container_scope(container: *mut DiContainer) -> *mut DiContainer {
    ffi_guard(ptr::null_mut(), || {
        if container.is_null() {
            set_last_error("Container pointer is null");
            return ptr::null_mut();
        }

        // SAFETY: Caller guarantees container is valid
        let parent = unsafe { &*container };

        // Clone the services map for the child scope. Recover from lock
        // poisoning here (and at every lock site below): poisoning cannot
        // leave the map in a torn state because keys are plain Strings and no
        // user code runs under the lock, so every critical section either
        // completes or leaves the map untouched. Recovering prevents one
        // caught panic from permanently bricking the container.
        let services = parent
            .services
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        let child = Box::new(DiContainer {
            services: RwLock::new(services),
            locked: AtomicBool::new(false),
        });
        Box::into_raw(child)
    })
}

// ============================================================================
// Service Registration
// ============================================================================

/// Register a singleton service with raw byte data.
///
/// # Arguments
/// - `container` - The container to register in
/// - `type_name` - A unique string identifier for this service type (null-terminated)
/// - `data` - Pointer to the service data bytes
/// - `data_len` - Length of the data in bytes
///
/// # Returns
/// Error code indicating success or failure. Returns `Locked` if the
/// container has been locked with `di_lock()`.
///
/// # Safety
/// - `container` must be a valid container pointer
/// - `type_name` must be a valid null-terminated UTF-8 string
/// - `data` must point to at least `data_len` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_register_singleton(
    container: *mut DiContainer,
    type_name: *const c_char,
    data: *const u8,
    data_len: usize,
) -> DiErrorCode {
    ffi_guard(DiErrorCode::InternalError, || {
        // Validate container
        if container.is_null() {
            set_last_error("Container pointer is null");
            return DiErrorCode::InvalidArgument;
        }

        // Validate type_name
        if type_name.is_null() {
            set_last_error("Type name is null");
            return DiErrorCode::InvalidArgument;
        }

        // SAFETY: Caller guarantees type_name is valid
        let type_name_str = if let Ok(s) = unsafe { CStr::from_ptr(type_name) }.to_str() {
            s.to_string()
        } else {
            set_last_error("Type name is not valid UTF-8");
            return DiErrorCode::InvalidArgument;
        };

        // Validate data
        if data.is_null() && data_len > 0 {
            set_last_error("Data pointer is null but length is non-zero");
            return DiErrorCode::InvalidArgument;
        }

        // Copy the data
        let data_vec = if data_len > 0 {
            // SAFETY: Caller guarantees data points to data_len bytes
            unsafe { std::slice::from_raw_parts(data, data_len) }.to_vec()
        } else {
            Vec::new()
        };

        // SAFETY: Caller guarantees container is valid
        let container = unsafe { &*container };

        // Match core semantics: locking blocks registration only.
        if container.locked.load(Ordering::Acquire) {
            set_last_error("Container is locked - cannot register new services");
            return DiErrorCode::Locked;
        }

        // Check and insert atomically under a single write lock so two threads
        // cannot both pass the existence check and silently overwrite each other.
        let mut services = container
            .services
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        match services.entry(type_name_str) {
            Entry::Occupied(entry) => {
                set_last_error(format!("Service '{}' is already registered", entry.key()));
                DiErrorCode::AlreadyRegistered
            }
            Entry::Vacant(entry) => {
                let service_data: Arc<dyn Any + Send + Sync> = Arc::new(data_vec);
                entry.insert(service_data);
                DiErrorCode::Ok
            }
        }
    })
}

/// Register a singleton service with a JSON string.
///
/// This is a convenience function for languages that prefer JSON serialization.
///
/// # Arguments
/// - `container` - The container to register in
/// - `type_name` - A unique string identifier for this service type
/// - `json_data` - JSON-serialized service data (null-terminated)
///
/// # Returns
/// Error code indicating success or failure. Returns `Locked` if the
/// container has been locked with `di_lock()`.
///
/// # Safety
/// - `container` must be a valid pointer to a `DiContainer` returned from `di_container_create`
/// - `type_name` must be a valid null-terminated C string
/// - `json_data` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_register_singleton_json(
    container: *mut DiContainer,
    type_name: *const c_char,
    json_data: *const c_char,
) -> DiErrorCode {
    ffi_guard(DiErrorCode::InternalError, || {
        if json_data.is_null() {
            set_last_error("JSON data is null");
            return DiErrorCode::InvalidArgument;
        }

        // SAFETY: Caller guarantees json_data is valid
        let json_str = match unsafe { CStr::from_ptr(json_data) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("JSON data is not valid UTF-8");
                return DiErrorCode::InvalidArgument;
            }
        };

        let json_bytes = json_str.as_bytes();

        // SAFETY: We just validated all pointers
        unsafe {
            di_register_singleton(container, type_name, json_bytes.as_ptr(), json_bytes.len())
        }
    })
}

// ============================================================================
// Service Removal and Locking
// ============================================================================

/// Remove a registered service by type name.
///
/// Matching the core container's semantics, removal is permitted on a locked
/// container: locking prevents new registrations only.
///
/// # Returns
/// `Ok` if the service was removed, `NotFound` if no service with that name
/// is registered (with the last error set), or `InvalidArgument` for a null
/// container, null type name, or a type name that is not valid UTF-8.
///
/// # Safety
/// - `container` must be a valid pointer to a `DiContainer`
/// - `type_name` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_remove(
    container: *mut DiContainer,
    type_name: *const c_char,
) -> DiErrorCode {
    ffi_guard(DiErrorCode::InternalError, || {
        // Validate container
        if container.is_null() {
            set_last_error("Container pointer is null");
            return DiErrorCode::InvalidArgument;
        }

        // Validate type_name
        if type_name.is_null() {
            set_last_error("Type name is null");
            return DiErrorCode::InvalidArgument;
        }

        // SAFETY: Caller guarantees type_name is valid
        let type_name_str = match unsafe { CStr::from_ptr(type_name) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("Type name is not valid UTF-8");
                return DiErrorCode::InvalidArgument;
            }
        };

        // SAFETY: Caller guarantees container is valid
        let container = unsafe { &*container };

        let removed = container
            .services
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(type_name_str);
        if removed.is_some() {
            DiErrorCode::Ok
        } else {
            set_last_error(format!("Service '{type_name_str}' not found"));
            DiErrorCode::NotFound
        }
    })
}

/// Remove all registered services from a container.
///
/// Matching the core container's semantics, clearing is permitted on a locked
/// container: locking prevents new registrations only.
///
/// # Returns
/// `Ok` on success, or `InvalidArgument` if the container is null.
///
/// # Safety
/// - `container` must be a valid pointer to a `DiContainer`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_clear(container: *mut DiContainer) -> DiErrorCode {
    ffi_guard(DiErrorCode::InternalError, || {
        if container.is_null() {
            set_last_error("Container pointer is null");
            return DiErrorCode::InvalidArgument;
        }

        // SAFETY: Caller guarantees container is valid
        let container = unsafe { &*container };
        container
            .services
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        DiErrorCode::Ok
    })
}

/// Lock a container to prevent further registrations.
///
/// Once locked, `di_register_singleton()` and `di_register_singleton_json()`
/// return `Locked`. Removal (`di_remove()`) and clearing (`di_clear()`)
/// remain permitted, matching the core container's semantics. There is no
/// unlock. Child scopes created with `di_container_scope()` start unlocked.
///
/// A null container is recorded as an error (see `di_error_message()`) and
/// otherwise ignored.
///
/// # Safety
/// - `container` must be a valid pointer to a `DiContainer`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_lock(container: *mut DiContainer) {
    ffi_guard((), || {
        if container.is_null() {
            set_last_error("Container pointer is null");
            return;
        }

        // SAFETY: Caller guarantees container is valid
        unsafe { &*container }.locked.store(true, Ordering::Release);
    });
}

/// Check whether a container is locked.
///
/// # Returns
/// 1 if the container is locked, 0 if it is not, -1 on error.
///
/// # Safety
/// - `container` must be a valid pointer to a `DiContainer`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_is_locked(container: *const DiContainer) -> i32 {
    ffi_guard(-1, || {
        if container.is_null() {
            return -1;
        }

        // SAFETY: Caller guarantees container is valid
        i32::from(unsafe { &*container }.locked.load(Ordering::Acquire))
    })
}

// ============================================================================
// Service Resolution
// ============================================================================

/// Resolve a service by type name.
///
/// # Arguments
/// - `container` - The container to resolve from
/// - `type_name` - The service type name to resolve
///
/// # Returns
/// A DiResult with the service handle on success, or an error code on failure.
///
/// # Safety
/// - `container` must be a valid container pointer
/// - `type_name` must be a valid null-terminated UTF-8 string
/// - On success, the returned service must be freed with `di_service_free()`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_resolve(
    container: *mut DiContainer,
    type_name: *const c_char,
) -> DiResult {
    let internal_error = DiResult {
        code: DiErrorCode::InternalError,
        service: ptr::null_mut(),
    };
    ffi_guard(internal_error, || {
        // Validate container
        if container.is_null() {
            set_last_error("Container pointer is null");
            return DiResult {
                code: DiErrorCode::InvalidArgument,
                service: ptr::null_mut(),
            };
        }

        // Validate type_name
        if type_name.is_null() {
            set_last_error("Type name is null");
            return DiResult {
                code: DiErrorCode::InvalidArgument,
                service: ptr::null_mut(),
            };
        }

        // SAFETY: Caller guarantees type_name is valid
        let type_name_str = match unsafe { CStr::from_ptr(type_name) }.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                set_last_error("Type name is not valid UTF-8");
                return DiResult {
                    code: DiErrorCode::InvalidArgument,
                    service: ptr::null_mut(),
                };
            }
        };

        // SAFETY: Caller guarantees container is valid
        let container = unsafe { &*container };

        // Look up the service
        let services = container
            .services
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        match services.get(&type_name_str) {
            Some(service_arc) => {
                // Downcast to Vec<u8>
                if let Some(data) = service_arc.downcast_ref::<Vec<u8>>() {
                    let service = Box::new(DiService {
                        type_name: type_name_str,
                        data: data.clone(),
                    });
                    DiResult {
                        code: DiErrorCode::Ok,
                        service: Box::into_raw(service),
                    }
                } else {
                    set_last_error("Internal error: service data type mismatch");
                    DiResult {
                        code: DiErrorCode::InternalError,
                        service: ptr::null_mut(),
                    }
                }
            }
            None => {
                set_last_error(format!("Service '{}' not found", type_name_str));
                DiResult {
                    code: DiErrorCode::NotFound,
                    service: ptr::null_mut(),
                }
            }
        }
    })
}

/// Resolve a service and return its data as a JSON string.
///
/// This is a convenience function for languages that use JSON serialization.
///
/// # Arguments
/// - `container` - The container to resolve from
/// - `type_name` - The service type name to resolve
///
/// # Returns
/// A pointer to the null-terminated JSON string, or NULL on any failure
/// (service not found, invalid arguments, non-UTF-8 service data, data
/// containing null bytes, or an internal error). Call `di_error_message()`
/// to distinguish causes. The pointer must be freed with `di_string_free()`.
///
/// # Safety
/// - `container` must be a valid container pointer
/// - `type_name` must be a valid null-terminated UTF-8 string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_resolve_json(
    container: *mut DiContainer,
    type_name: *const c_char,
) -> *mut c_char {
    ffi_guard(ptr::null_mut(), || {
        // Validate container
        if container.is_null() {
            set_last_error("Container pointer is null");
            return ptr::null_mut();
        }

        // Validate type_name
        if type_name.is_null() {
            set_last_error("Type name is null");
            return ptr::null_mut();
        }

        // SAFETY: Caller guarantees type_name is valid
        let type_name_str = match unsafe { CStr::from_ptr(type_name) }.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                set_last_error("Type name is not valid UTF-8");
                return ptr::null_mut();
            }
        };

        // SAFETY: Caller guarantees container is valid
        let container = unsafe { &*container };

        // Look up the service
        let services = container
            .services
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        match services.get(&type_name_str) {
            Some(service_arc) => {
                // Downcast to Vec<u8>
                if let Some(data) = service_arc.downcast_ref::<Vec<u8>>() {
                    // Convert bytes to string (assuming UTF-8 JSON)
                    match std::str::from_utf8(data) {
                        Ok(json_str) => match CString::new(json_str) {
                            Ok(cstr) => cstr.into_raw(),
                            Err(_) => {
                                set_last_error("JSON string contains null bytes");
                                ptr::null_mut()
                            }
                        },
                        Err(_) => {
                            set_last_error("Service data is not valid UTF-8");
                            ptr::null_mut()
                        }
                    }
                } else {
                    set_last_error("Internal error: service data type mismatch");
                    ptr::null_mut()
                }
            }
            None => {
                set_last_error(format!("Service '{}' not found", type_name_str));
                ptr::null_mut()
            }
        }
    })
}

/// Check if a service is registered.
///
/// # Returns
/// 1 if the service is registered, 0 if not, -1 on error.
///
/// # Safety
/// - `container` must be a valid pointer to a `DiContainer`
/// - `type_name` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_contains(container: *mut DiContainer, type_name: *const c_char) -> i32 {
    ffi_guard(-1, || {
        if container.is_null() || type_name.is_null() {
            return -1;
        }

        // SAFETY: Caller guarantees type_name is valid
        let type_name_str = match unsafe { CStr::from_ptr(type_name) }.to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        };

        // SAFETY: Caller guarantees container is valid
        let container = unsafe { &*container };
        let services = container
            .services
            .read()
            .unwrap_or_else(PoisonError::into_inner);

        if services.contains_key(type_name_str) {
            1
        } else {
            0
        }
    })
}

// ============================================================================
// Service Data Access
// ============================================================================

/// Get the data pointer from a service handle.
///
/// # Returns
/// Pointer to the service data, or NULL on error.
/// The pointer is valid until the service is freed.
///
/// # Safety
/// - `service` must be a valid pointer to a `DiService` returned from `di_resolve`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_service_data(service: *const DiService) -> *const u8 {
    ffi_guard(ptr::null(), || {
        if service.is_null() {
            return ptr::null();
        }
        // SAFETY: Caller guarantees service is valid
        unsafe { &*service }.data.as_ptr()
    })
}

/// Get the data length from a service handle.
///
/// # Returns
/// Length of the service data in bytes, or 0 on error.
///
/// # Safety
/// - `service` must be a valid pointer to a `DiService` returned from `di_resolve`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_service_data_len(service: *const DiService) -> usize {
    ffi_guard(0, || {
        if service.is_null() {
            return 0;
        }
        // SAFETY: Caller guarantees service is valid
        unsafe { &*service }.data.len()
    })
}

/// Get the type name from a service handle.
///
/// # Returns
/// Pointer to the null-terminated type name, or NULL on error.
/// The returned string is a fresh allocation owned by the caller and must be
/// freed with `di_string_free()`.
///
/// # Safety
/// - `service` must be a valid pointer to a `DiService` returned from `di_resolve`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_service_type_name(service: *const DiService) -> *const c_char {
    ffi_guard(ptr::null(), || {
        if service.is_null() {
            return ptr::null();
        }
        // SAFETY: Caller guarantees service is valid
        let service = unsafe { &*service };

        // Create a CString and leak it - caller must free with di_string_free
        match CString::new(service.type_name.clone()) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => ptr::null(),
        }
    })
}

/// Free a service handle.
///
/// # Safety
/// - `service` must be a valid pointer returned by `di_resolve()`
/// - After calling this function, the pointer is invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_service_free(service: *mut DiService) {
    ffi_guard((), || {
        if !service.is_null() {
            // SAFETY: Caller guarantees service is valid
            drop(unsafe { Box::from_raw(service) });
        }
    });
}

// ============================================================================
// Error Handling
// ============================================================================

/// Get the last error message.
///
/// # Returns
/// A pointer to the null-terminated error message, or NULL if no error.
/// The pointer must be freed with `di_string_free()`.
#[unsafe(no_mangle)]
pub extern "C" fn di_error_message() -> *mut c_char {
    ffi_guard(ptr::null_mut(), || {
        LAST_ERROR.with(|e| {
            let error = e.borrow();
            match &*error {
                Some(msg) => match CString::new(msg.as_str()) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => ptr::null_mut(),
                },
                None => ptr::null_mut(),
            }
        })
    })
}

/// Clear the last error message.
#[unsafe(no_mangle)]
pub extern "C" fn di_error_clear() {
    ffi_guard((), || {
        LAST_ERROR.with(|e| {
            *e.borrow_mut() = None;
        });
    });
}

/// Free a string returned by the library.
///
/// # Safety
/// - `s` must be a string returned by this library (e.g., from `di_error_message()`)
/// - After calling this function, the pointer is invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_string_free(s: *mut c_char) {
    ffi_guard((), || {
        if !s.is_null() {
            // SAFETY: Caller guarantees s was allocated by CString::into_raw
            drop(unsafe { CString::from_raw(s) });
        }
    });
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Get the library version.
///
/// # Returns
/// A pointer to the null-terminated version string.
/// This string is statically allocated and must NOT be freed.
#[unsafe(no_mangle)]
pub extern "C" fn di_version() -> *const c_char {
    ffi_guard(ptr::null(), || {
        static VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
        VERSION.as_ptr() as *const c_char
    })
}

/// Get the number of registered services in a container.
///
/// # Returns
/// The number of services, or -1 on error.
///
/// # Safety
/// - `container` must be a valid pointer to a `DiContainer`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_service_count(container: *const DiContainer) -> i64 {
    ffi_guard(-1, || {
        if container.is_null() {
            return -1;
        }
        // SAFETY: Caller guarantees container is valid
        let container = unsafe { &*container };
        let services = container
            .services
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        services.len() as i64
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `f` against a fresh container, always freeing it afterwards.
    fn with_container(f: impl FnOnce(*mut DiContainer)) {
        let container = di_container_new();
        assert!(!container.is_null());
        f(container);
        // SAFETY: `container` came from `di_container_new` and is freed
        // exactly once, after `f` has finished using it.
        unsafe { di_container_free(container) };
    }

    /// Fetch the last error message as an owned `String`, asserting one is
    /// present and freeing the FFI allocation per the ABI contract.
    fn last_error_string() -> String {
        let error = di_error_message();
        assert!(!error.is_null(), "expected a last error message");
        // SAFETY: `error` is a valid, non-null string allocated by
        // `di_error_message`; it is freed exactly once below.
        let message = unsafe { CStr::from_ptr(error) }
            .to_str()
            .unwrap()
            .to_owned();
        // SAFETY: `error` came from `di_error_message` and is freed once.
        unsafe { di_string_free(error) };
        message
    }

    #[test]
    fn test_ffi_guard_catches_str_panic() {
        di_error_clear();
        assert_eq!(panicking_entry_str(), DiErrorCode::InternalError);
        let msg = last_error_string();
        assert!(msg.contains("internal panic"), "got: {msg}");
        assert!(msg.contains("boom"), "got: {msg}");
    }

    #[test]
    fn test_ffi_guard_catches_string_panic() {
        di_error_clear();
        assert_eq!(panicking_entry_string(), DiErrorCode::InternalError);
        let msg = last_error_string();
        assert!(msg.contains("internal panic"), "got: {msg}");
        assert!(msg.contains("boom-string"), "got: {msg}");
    }

    #[test]
    fn test_container_lifecycle() {
        unsafe {
            let container = di_container_new();
            assert!(!container.is_null());
            di_container_free(container);
        }
    }

    #[test]
    fn test_register_and_resolve() {
        with_container(|container| unsafe {
            let type_name = CString::new("TestService").unwrap();
            let data = b"hello world";

            let result =
                di_register_singleton(container, type_name.as_ptr(), data.as_ptr(), data.len());
            assert_eq!(result, DiErrorCode::Ok);

            let resolve_result = di_resolve(container, type_name.as_ptr());
            assert_eq!(resolve_result.code, DiErrorCode::Ok);
            assert!(!resolve_result.service.is_null());

            let service = resolve_result.service;
            assert_eq!(di_service_data_len(service), 11);

            let data_ptr = di_service_data(service);
            let resolved_data = std::slice::from_raw_parts(data_ptr, 11);
            assert_eq!(resolved_data, b"hello world");

            di_service_free(service);
        });
    }

    #[test]
    fn test_not_found() {
        with_container(|container| unsafe {
            let type_name = CString::new("NonExistent").unwrap();

            let result = di_resolve(container, type_name.as_ptr());
            assert_eq!(result.code, DiErrorCode::NotFound);
            assert!(result.service.is_null());
        });
    }

    #[test]
    fn test_contains() {
        with_container(|container| unsafe {
            let type_name = CString::new("TestService").unwrap();

            assert_eq!(di_contains(container, type_name.as_ptr()), 0);

            let data = b"test";
            di_register_singleton(container, type_name.as_ptr(), data.as_ptr(), data.len());

            assert_eq!(di_contains(container, type_name.as_ptr()), 1);
        });
    }

    #[test]
    fn test_scope() {
        unsafe {
            let parent = di_container_new();
            let type_name = CString::new("ParentService").unwrap();
            let data = b"parent";

            di_register_singleton(parent, type_name.as_ptr(), data.as_ptr(), data.len());

            let child = di_container_scope(parent);
            assert!(!child.is_null());

            // Child should inherit parent's services
            assert_eq!(di_contains(child, type_name.as_ptr()), 1);

            di_container_free(child);
            di_container_free(parent);
        }
    }

    #[test]
    fn test_duplicate_registration() {
        with_container(|container| unsafe {
            let type_name = CString::new("Duplicate").unwrap();
            let data = b"data";

            let first =
                di_register_singleton(container, type_name.as_ptr(), data.as_ptr(), data.len());
            assert_eq!(first, DiErrorCode::Ok);

            let second =
                di_register_singleton(container, type_name.as_ptr(), data.as_ptr(), data.len());
            assert_eq!(second, DiErrorCode::AlreadyRegistered);
        });
    }

    #[test]
    fn test_concurrent_duplicate_registration() {
        // Regression test for the check-then-insert TOCTOU: registration is
        // atomic under a single write lock, so of N threads racing to
        // register the same name, exactly one must win per round.
        struct SendPtr(*mut DiContainer);
        // SAFETY: DiContainer's internals are RwLock-protected and all FFI
        // functions are documented as thread-safe.
        unsafe impl Send for SendPtr {}
        unsafe impl Sync for SendPtr {}

        const THREADS: usize = 8;
        const ROUNDS: usize = 100;

        let container = SendPtr(di_container_new());
        let container = std::sync::Arc::new(container);

        for round in 0..ROUNDS {
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(THREADS));
            let name = std::sync::Arc::new(CString::new(format!("Svc{round}")).unwrap());

            let handles: Vec<_> = (0..THREADS)
                .map(|_| {
                    let container = std::sync::Arc::clone(&container);
                    let barrier = std::sync::Arc::clone(&barrier);
                    let name = std::sync::Arc::clone(&name);
                    std::thread::spawn(move || {
                        let data = b"race";
                        barrier.wait();
                        unsafe {
                            di_register_singleton(
                                container.0,
                                name.as_ptr(),
                                data.as_ptr(),
                                data.len(),
                            )
                        }
                    })
                })
                .collect();

            let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            let ok = results.iter().filter(|r| **r == DiErrorCode::Ok).count();
            let dup = results
                .iter()
                .filter(|r| **r == DiErrorCode::AlreadyRegistered)
                .count();
            assert_eq!(ok, 1, "round {round}: exactly one registration must win");
            assert_eq!(
                dup,
                THREADS - 1,
                "round {round}: the rest must see AlreadyRegistered"
            );
        }

        unsafe { di_container_free(container.0) };
    }

    #[test]
    fn test_concurrent_lock_vs_registration() {
        // di_lock racing di_register_singleton: every registration attempt
        // must resolve cleanly to Ok (beat the lock) or Locked (lost to it),
        // and once the lock has landed it must refuse all later attempts.
        struct SendPtr(*mut DiContainer);
        // SAFETY: DiContainer's internals are RwLock/atomic-protected and all
        // FFI functions are documented as thread-safe.
        unsafe impl Send for SendPtr {}
        unsafe impl Sync for SendPtr {}

        const THREADS: usize = 8;

        let container = std::sync::Arc::new(SendPtr(di_container_new()));
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(THREADS + 1));

        let locker = {
            let container = std::sync::Arc::clone(&container);
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                // SAFETY: the container stays alive until every thread below
                // has been joined.
                unsafe { di_lock(container.0) };
            })
        };

        let handles: Vec<_> = (0..THREADS)
            .map(|i| {
                let container = std::sync::Arc::clone(&container);
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let name = CString::new(format!("Racer{i}")).unwrap();
                    let data = b"race";
                    barrier.wait();
                    unsafe {
                        di_register_singleton(container.0, name.as_ptr(), data.as_ptr(), data.len())
                    }
                })
            })
            .collect();

        locker.join().unwrap();
        for handle in handles {
            let result = handle.join().unwrap();
            assert!(
                result == DiErrorCode::Ok || result == DiErrorCode::Locked,
                "registration racing di_lock must be Ok or Locked, got {result:?}"
            );
        }

        unsafe {
            // The lock has landed: every subsequent registration - raced
            // names and fresh ones alike - must now report Locked.
            assert_eq!(di_is_locked(container.0), 1);
            let data = b"race";
            for i in 0..THREADS {
                let name = CString::new(format!("Racer{i}")).unwrap();
                let result =
                    di_register_singleton(container.0, name.as_ptr(), data.as_ptr(), data.len());
                assert_eq!(result, DiErrorCode::Locked);
            }
            let fresh = CString::new("LateRacer").unwrap();
            let result =
                di_register_singleton(container.0, fresh.as_ptr(), data.as_ptr(), data.len());
            assert_eq!(result, DiErrorCode::Locked);
            di_container_free(container.0);
        }
    }

    #[test]
    fn test_json_register_and_resolve() {
        with_container(|container| unsafe {
            let type_name = CString::new("JsonService").unwrap();
            let json = CString::new("{\"name\":\"test\"}").unwrap();

            let result = di_register_singleton_json(container, type_name.as_ptr(), json.as_ptr());
            assert_eq!(result, DiErrorCode::Ok);

            let resolved = di_resolve_json(container, type_name.as_ptr());
            assert!(!resolved.is_null());
            assert_eq!(
                CStr::from_ptr(resolved).to_str().unwrap(),
                "{\"name\":\"test\"}"
            );
            di_string_free(resolved);
        });
    }

    #[test]
    fn test_resolve_json_not_found() {
        with_container(|container| unsafe {
            let type_name = CString::new("Missing").unwrap();

            di_error_clear();
            let resolved = di_resolve_json(container, type_name.as_ptr());
            assert!(resolved.is_null());

            let error = di_error_message();
            assert!(!error.is_null());
            assert!(
                CStr::from_ptr(error)
                    .to_str()
                    .unwrap()
                    .contains("not found")
            );
            di_string_free(error);
        });
    }

    #[test]
    fn test_service_type_name() {
        with_container(|container| unsafe {
            let type_name = CString::new("NamedService").unwrap();
            let data = b"data";

            di_register_singleton(container, type_name.as_ptr(), data.as_ptr(), data.len());

            let result = di_resolve(container, type_name.as_ptr());
            assert_eq!(result.code, DiErrorCode::Ok);

            let name = di_service_type_name(result.service);
            assert!(!name.is_null());
            assert_eq!(CStr::from_ptr(name).to_str().unwrap(), "NamedService");
            di_string_free(name.cast_mut());

            di_service_free(result.service);
        });
    }

    #[test]
    fn test_error_message_set_and_clear() {
        with_container(|container| unsafe {
            let type_name = CString::new("Missing").unwrap();

            let result = di_resolve(container, type_name.as_ptr());
            assert_eq!(result.code, DiErrorCode::NotFound);

            let error = di_error_message();
            assert!(!error.is_null());
            di_string_free(error);

            di_error_clear();
            assert!(di_error_message().is_null());
        });
    }

    #[test]
    fn test_version() {
        unsafe {
            let version = di_version();
            assert!(!version.is_null());
            assert_eq!(
                CStr::from_ptr(version).to_str().unwrap(),
                env!("CARGO_PKG_VERSION")
            );
        }
    }

    #[test]
    fn test_service_count() {
        with_container(|container| unsafe {
            assert_eq!(di_service_count(container), 0);

            let first = CString::new("First").unwrap();
            let second = CString::new("Second").unwrap();
            let data = b"data";

            di_register_singleton(container, first.as_ptr(), data.as_ptr(), data.len());
            assert_eq!(di_service_count(container), 1);

            di_register_singleton(container, second.as_ptr(), data.as_ptr(), data.len());
            assert_eq!(di_service_count(container), 2);
        });
    }

    #[test]
    fn test_remove_round_trip() {
        with_container(|container| unsafe {
            let type_name = CString::new("Removable").unwrap();
            let data = b"data";

            di_register_singleton(container, type_name.as_ptr(), data.as_ptr(), data.len());
            assert_eq!(di_contains(container, type_name.as_ptr()), 1);

            assert_eq!(di_remove(container, type_name.as_ptr()), DiErrorCode::Ok);
            assert_eq!(di_contains(container, type_name.as_ptr()), 0);

            // Removing again must report NotFound, naming the service.
            assert_eq!(
                di_remove(container, type_name.as_ptr()),
                DiErrorCode::NotFound
            );
            let msg = last_error_string();
            assert!(msg.contains("Removable"), "got: {msg}");
        });
    }

    #[test]
    fn test_clear() {
        with_container(|container| unsafe {
            let first = CString::new("First").unwrap();
            let second = CString::new("Second").unwrap();
            let data = b"data";

            di_register_singleton(container, first.as_ptr(), data.as_ptr(), data.len());
            di_register_singleton(container, second.as_ptr(), data.as_ptr(), data.len());
            assert_eq!(di_service_count(container), 2);

            assert_eq!(di_clear(container), DiErrorCode::Ok);
            assert_eq!(di_service_count(container), 0);
        });
    }

    #[test]
    fn test_lock_blocks_registration_but_allows_remove_and_clear() {
        with_container(|container| unsafe {
            let existing = CString::new("Existing").unwrap();
            let blocked = CString::new("Blocked").unwrap();
            let data = b"data";

            di_register_singleton(container, existing.as_ptr(), data.as_ptr(), data.len());

            di_lock(container);

            let result =
                di_register_singleton(container, blocked.as_ptr(), data.as_ptr(), data.len());
            assert_eq!(result, DiErrorCode::Locked);

            // A Locked failure must surface a message mentioning the lock.
            let msg = last_error_string();
            assert!(msg.contains("locked"), "got: {msg}");

            let json = CString::new("{}").unwrap();
            let json_result =
                di_register_singleton_json(container, blocked.as_ptr(), json.as_ptr());
            assert_eq!(json_result, DiErrorCode::Locked);

            // Locking blocks registration only; removal and clearing still work.
            assert_eq!(di_remove(container, existing.as_ptr()), DiErrorCode::Ok);
            assert_eq!(di_clear(container), DiErrorCode::Ok);
        });
    }

    #[test]
    fn test_is_locked_transitions() {
        with_container(|container| unsafe {
            assert_eq!(di_is_locked(container), 0);
            di_lock(container);
            assert_eq!(di_is_locked(container), 1);
        });
    }

    #[test]
    fn test_null_container_sentinels() {
        unsafe {
            // -1 is the documented error sentinel for both query functions;
            // it must stay distinct from the 0 ("false") result.
            assert_eq!(di_is_locked(std::ptr::null_mut()), -1);
            let name = CString::new("Anything").unwrap();
            assert_eq!(di_contains(std::ptr::null_mut(), name.as_ptr()), -1);
        }
    }

    #[test]
    fn test_error_code_from_core_error() {
        assert_eq!(
            DiErrorCode::from(&DiError::not_found::<u32>()),
            DiErrorCode::NotFound
        );
        assert_eq!(
            DiErrorCode::from(&DiError::already_registered::<u32>()),
            DiErrorCode::AlreadyRegistered
        );
        assert_eq!(DiErrorCode::from(&DiError::Locked), DiErrorCode::Locked);
        assert_eq!(
            DiErrorCode::from(&DiError::circular::<u32>()),
            DiErrorCode::InternalError
        );
        assert_eq!(
            DiErrorCode::from(&DiError::Internal(String::from("boom"))),
            DiErrorCode::InternalError
        );
    }
}

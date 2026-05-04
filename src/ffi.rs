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

use std::any::Any;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::ptr;
use std::sync::{Arc, RwLock};

/// Opaque container handle for FFI
pub struct DiContainer {
    inner: crate::Container,
    /// Map of type names to their registered services (as raw bytes)
    services: RwLock<HashMap<String, Arc<dyn Any + Send + Sync>>>,
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

fn set_last_error(msg: impl Into<String>) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = Some(msg.into());
    });
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
    let container = Box::new(DiContainer {
        inner: crate::Container::new(),
        services: RwLock::new(HashMap::new()),
    });
    Box::into_raw(container)
}

/// Free a container and all its resources.
///
/// # Safety
/// - `container` must be a valid pointer returned by `di_container_new()`
/// - After calling this function, the pointer is invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_container_free(container: *mut DiContainer) {
    if !container.is_null() {
        // SAFETY: Caller guarantees container is valid
        drop(unsafe { Box::from_raw(container) });
    }
}

/// Create a child scope from a container.
///
/// # Returns
/// A pointer to the new scoped container, or NULL on failure.
///
/// # Safety
/// - `container` must be a valid container pointer
/// - The returned pointer must be freed with `di_container_free()`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_container_scope(container: *mut DiContainer) -> *mut DiContainer {
    if container.is_null() {
        set_last_error("Container pointer is null");
        return ptr::null_mut();
    }

    // SAFETY: Caller guarantees container is valid
    let parent = unsafe { &*container };
    let child_inner = parent.inner.scope();

    // Clone the services map for the child scope
    let services = parent.services.read().unwrap().clone();

    let child = Box::new(DiContainer {
        inner: child_inner,
        services: RwLock::new(services),
    });
    Box::into_raw(child)
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
/// Error code indicating success or failure.
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

    // Check if already registered
    {
        let services = container.services.read().unwrap();
        if services.contains_key(&type_name_str) {
            set_last_error(format!("Service '{}' is already registered", type_name_str));
            return DiErrorCode::AlreadyRegistered;
        }
    }

    // Store the service data
    let service_data: Arc<dyn Any + Send + Sync> = Arc::new(data_vec);
    container
        .services
        .write()
        .unwrap()
        .insert(type_name_str, service_data);

    DiErrorCode::Ok
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
/// Error code indicating success or failure.
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
    unsafe { di_register_singleton(container, type_name, json_bytes.as_ptr(), json_bytes.len()) }
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
    let services = container.services.read().unwrap();
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
/// A pointer to the null-terminated JSON string, or NULL if not found.
/// The pointer must be freed with `di_string_free()`.
///
/// # Safety
/// - `container` must be a valid container pointer
/// - `type_name` must be a valid null-terminated UTF-8 string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_resolve_json(
    container: *mut DiContainer,
    type_name: *const c_char,
) -> *mut c_char {
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
    let services = container.services.read().unwrap();
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
    let services = container.services.read().unwrap();

    if services.contains_key(type_name_str) {
        1
    } else {
        0
    }
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
    if service.is_null() {
        return ptr::null();
    }
    // SAFETY: Caller guarantees service is valid
    unsafe { &*service }.data.as_ptr()
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
    if service.is_null() {
        return 0;
    }
    // SAFETY: Caller guarantees service is valid
    unsafe { &*service }.data.len()
}

/// Get the type name from a service handle.
///
/// # Returns
/// Pointer to the null-terminated type name, or NULL on error.
/// The pointer is valid until the service is freed.
///
/// # Safety
/// - `service` must be a valid pointer to a `DiService` returned from `di_resolve`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_service_type_name(service: *const DiService) -> *const c_char {
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
}

/// Free a service handle.
///
/// # Safety
/// - `service` must be a valid pointer returned by `di_resolve()`
/// - After calling this function, the pointer is invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_service_free(service: *mut DiService) {
    if !service.is_null() {
        // SAFETY: Caller guarantees service is valid
        drop(unsafe { Box::from_raw(service) });
    }
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
}

/// Clear the last error message.
#[unsafe(no_mangle)]
pub extern "C" fn di_error_clear() {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = None;
    });
}

/// Free a string returned by the library.
///
/// # Safety
/// - `s` must be a string returned by this library (e.g., from `di_error_message()`)
/// - After calling this function, the pointer is invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn di_string_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: Caller guarantees s was allocated by CString::into_raw
        drop(unsafe { CString::from_raw(s) });
    }
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
    static VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
    VERSION.as_ptr() as *const c_char
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
    if container.is_null() {
        return -1;
    }
    // SAFETY: Caller guarantees container is valid
    let container = unsafe { &*container };
    container.services.read().unwrap().len() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

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
        unsafe {
            let container = di_container_new();

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
            di_container_free(container);
        }
    }

    #[test]
    fn test_not_found() {
        unsafe {
            let container = di_container_new();
            let type_name = CString::new("NonExistent").unwrap();

            let result = di_resolve(container, type_name.as_ptr());
            assert_eq!(result.code, DiErrorCode::NotFound);
            assert!(result.service.is_null());

            di_container_free(container);
        }
    }

    #[test]
    fn test_contains() {
        unsafe {
            let container = di_container_new();
            let type_name = CString::new("TestService").unwrap();

            assert_eq!(di_contains(container, type_name.as_ptr()), 0);

            let data = b"test";
            di_register_singleton(container, type_name.as_ptr(), data.as_ptr(), data.len());

            assert_eq!(di_contains(container, type_name.as_ptr()), 1);

            di_container_free(container);
        }
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
}

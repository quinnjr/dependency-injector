/**
 * dependency-injector FFI bindings
 *
 * High-performance dependency injection container for Rust, accessible from C/Go/Python.
 *
 * ## Memory Management
 *
 * - `di_container_new()` allocates a container - must be freed with `di_container_free()`
 * - `di_resolve()` returns a service handle - must be freed with `di_service_free()`
 * - `di_error_message()` returns a string - must be freed with `di_string_free()`
 *
 * ## Thread Safety
 *
 * The container is thread-safe. All functions can be called from multiple threads.
 *
 * ## Panic Safety
 *
 * Panics inside the library are caught at the FFI boundary and never unwind
 * into the caller. When a panic occurs, the function returns its documented
 * error value (NULL, an error code, -1, or 0 depending on the function) and
 * the panic message is surfaced via di_error_message().
 *
 * ## Example (C)
 *
 * ```c
 * DiContainer* container = di_container_new();
 *
 * const char* data = "{\"name\": \"MyService\"}";
 * di_register_singleton_json(container, "MyService", data);
 *
 * DiResult result = di_resolve(container, "MyService");
 * if (result.code == DI_OK) {
 *     const uint8_t* service_data = di_service_data(result.service);
 *     size_t len = di_service_data_len(result.service);
 *     // Use service_data...
 *     di_service_free(result.service);
 * }
 *
 * di_container_free(container);
 * ```
 */

#ifndef DEPENDENCY_INJECTOR_H
#define DEPENDENCY_INJECTOR_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Types
 * ============================================================================ */

/**
 * Opaque container handle.
 */
typedef struct DiContainer DiContainer;

/**
 * Opaque service handle.
 */
typedef struct DiService DiService;

/**
 * Error codes returned by FFI functions.
 */
typedef enum DiErrorCode {
    /** Operation succeeded */
    DI_OK = 0,
    /** Service not found */
    DI_NOT_FOUND = 1,
    /** Invalid argument (null pointer, invalid UTF-8, etc.) */
    DI_INVALID_ARGUMENT = 2,
    /** Service already registered */
    DI_ALREADY_REGISTERED = 3,
    /** Internal error */
    DI_INTERNAL_ERROR = 4,
    /** Serialization/deserialization error */
    DI_SERIALIZATION_ERROR = 5,
    /** Container is locked - registration is not allowed */
    DI_LOCKED = 6,
} DiErrorCode;

/**
 * Result type for resolve operations.
 */
typedef struct DiResult {
    DiErrorCode code;
    DiService* service;
} DiResult;

/* ============================================================================
 * Container Lifecycle
 * ============================================================================ */

/**
 * Create a new dependency injection container.
 *
 * @return A pointer to the new container, or NULL on failure.
 *         Must be freed with di_container_free().
 */
DiContainer* di_container_new(void);

/**
 * Free a container and all its resources.
 *
 * @param container The container to free (may be NULL).
 */
void di_container_free(DiContainer* container);

/**
 * Create a child scope from a container.
 *
 * Inheritance is a snapshot taken at creation time: the child receives a
 * copy of the parent's services as they exist when the scope is created.
 * Services registered in the parent afterwards are not visible to existing
 * child scopes. The child starts unlocked regardless of the parent's lock
 * state, matching the core container's scoping semantics.
 *
 * @param container The parent container.
 * @return A pointer to the new scoped container, or NULL on failure.
 *         Must be freed with di_container_free().
 */
DiContainer* di_container_scope(DiContainer* container);

/* ============================================================================
 * Service Registration
 * ============================================================================ */

/**
 * Register a singleton service with raw byte data.
 *
 * @param container The container to register in.
 * @param type_name A unique string identifier for this service type (null-terminated).
 * @param data Pointer to the service data bytes.
 * @param data_len Length of the data in bytes.
 * @return Error code indicating success or failure. Returns DI_LOCKED if the
 *         container has been locked with di_lock().
 */
DiErrorCode di_register_singleton(
    DiContainer* container,
    const char* type_name,
    const uint8_t* data,
    size_t data_len
);

/**
 * Register a singleton service with a JSON string.
 *
 * Convenience function for languages that prefer JSON serialization.
 *
 * @param container The container to register in.
 * @param type_name A unique string identifier for this service type.
 * @param json_data JSON-serialized service data (null-terminated).
 * @return Error code indicating success or failure. Returns DI_LOCKED if the
 *         container has been locked with di_lock().
 */
DiErrorCode di_register_singleton_json(
    DiContainer* container,
    const char* type_name,
    const char* json_data
);

/* ============================================================================
 * Service Removal and Locking
 * ============================================================================ */

/**
 * Remove a registered service by type name.
 *
 * Matching the core container's semantics, removal is permitted on a locked
 * container: locking prevents new registrations only.
 *
 * @param container The container to remove from.
 * @param type_name The service type name to remove.
 * @return DI_OK if the service was removed, DI_NOT_FOUND if no service with
 *         that name is registered (with the last error set), or
 *         DI_INVALID_ARGUMENT for a null container, null type name, or a
 *         type name that is not valid UTF-8.
 */
DiErrorCode di_remove(DiContainer* container, const char* type_name);

/**
 * Remove all registered services from a container.
 *
 * Matching the core container's semantics, clearing is permitted on a locked
 * container: locking prevents new registrations only.
 *
 * @param container The container to clear.
 * @return DI_OK on success, or DI_INVALID_ARGUMENT if the container is null.
 */
DiErrorCode di_clear(DiContainer* container);

/**
 * Lock a container to prevent further registrations.
 *
 * Once locked, di_register_singleton() and di_register_singleton_json()
 * return DI_LOCKED. Removal (di_remove()) and clearing (di_clear()) remain
 * permitted, matching the core container's semantics: locking blocks
 * registration only. There is no unlock. Child scopes created with
 * di_container_scope() start unlocked.
 *
 * A null container is recorded as an error (see di_error_message()) and
 * otherwise ignored.
 *
 * @param container The container to lock.
 */
void di_lock(DiContainer* container);

/**
 * Check whether a container is locked.
 *
 * @param container The container to query.
 * @return 1 if the container is locked, 0 if it is not, -1 on error.
 *         A negative return (-1) signals an internal error or invalid
 *         argument - consult di_error_message() - and callers should not
 *         collapse it into "false". Note: no language binding exposes this
 *         function yet, so any new binding should surface the -1 case
 *         explicitly rather than folding it into a boolean.
 */
int32_t di_is_locked(const DiContainer* container);

/* ============================================================================
 * Service Resolution
 * ============================================================================ */

/**
 * Resolve a service by type name.
 *
 * @param container The container to resolve from.
 * @param type_name The service type name to resolve.
 * @return A DiResult with the service handle on success, or an error code on failure.
 *         On success, the service must be freed with di_service_free().
 */
DiResult di_resolve(DiContainer* container, const char* type_name);

/**
 * Resolve a service and return its data as a JSON string.
 *
 * Convenience function for languages that use JSON serialization.
 *
 * @param container The container to resolve from.
 * @param type_name The service type name to resolve.
 * @return A pointer to the null-terminated JSON string, or NULL on any
 *         failure (service not found, invalid arguments, non-UTF-8 service
 *         data, data containing null bytes, or an internal error). Call
 *         di_error_message() to distinguish causes.
 *         Must be freed with di_string_free().
 */
char* di_resolve_json(DiContainer* container, const char* type_name);

/**
 * Check if a service is registered.
 *
 * @param container The container to check.
 * @param type_name The service type name to check.
 * @return 1 if registered, 0 if not, -1 on error.
 *         A negative return (-1) signals an internal error or invalid
 *         argument - consult di_error_message() - and callers should not
 *         collapse it into "false". Note: the current language bindings
 *         treat -1 as false (known limitation).
 */
int32_t di_contains(DiContainer* container, const char* type_name);

/* ============================================================================
 * Service Data Access
 * ============================================================================ */

/**
 * Get the data pointer from a service handle.
 *
 * @param service The service handle.
 * @return Pointer to the service data, or NULL on error.
 *         Valid until the service is freed.
 */
const uint8_t* di_service_data(const DiService* service);

/**
 * Get the data length from a service handle.
 *
 * @param service The service handle.
 * @return Length of the service data in bytes, or 0 on error.
 */
size_t di_service_data_len(const DiService* service);

/**
 * Get the type name from a service handle.
 *
 * @param service The service handle.
 * @return Pointer to the null-terminated type name, or NULL on error.
 *         Must be freed with di_string_free().
 */
const char* di_service_type_name(const DiService* service);

/**
 * Free a service handle.
 *
 * @param service The service to free (may be NULL).
 */
void di_service_free(DiService* service);

/* ============================================================================
 * Error Handling
 * ============================================================================ */

/**
 * Get the last error message (thread-local).
 *
 * @return A pointer to the error message, or NULL if no error.
 *         Must be freed with di_string_free().
 */
char* di_error_message(void);

/**
 * Clear the last error message.
 */
void di_error_clear(void);

/**
 * Free a string returned by the library.
 *
 * @param s The string to free (may be NULL).
 */
void di_string_free(char* s);

/* ============================================================================
 * Utility Functions
 * ============================================================================ */

/**
 * Get the library version.
 *
 * @return A pointer to the null-terminated version string.
 *         This is statically allocated and must NOT be freed.
 */
const char* di_version(void);

/**
 * Get the number of registered services in a container.
 *
 * @param container The container to query.
 * @return The number of services, or -1 on error.
 */
int64_t di_service_count(const DiContainer* container);

#ifdef __cplusplus
}
#endif

#endif /* DEPENDENCY_INJECTOR_H */


// Package di provides Go bindings for the dependency-injector Rust library.
//
// This package wraps the high-performance Rust dependency injection container,
// making it available for Go applications via cgo.
//
// # Building
//
// First, build the Rust library:
//
//	cd /path/to/dependency-injector
//	cargo rustc --release --features ffi --crate-type cdylib
//
// Then set the library path:
//
//	export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
//	# or on macOS:
//	export DYLD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$DYLD_LIBRARY_PATH
//
// # Example
//
//	container := di.NewContainer()
//	defer container.Free()
//
//	// Register a service as JSON
//	err := container.RegisterJSON("UserService", `{"id": 1, "name": "Alice"}`)
//	if err != nil {
//	    log.Fatal(err)
//	}
//
//	// Resolve the service
//	data, err := container.ResolveJSON("UserService")
//	if err != nil {
//	    log.Fatal(err)
//	}
//	fmt.Println(string(data)) // {"id": 1, "name": "Alice"}
package di

/*
#cgo LDFLAGS: -L${SRCDIR}/../../../target/release -ldependency_injector
#cgo CFLAGS: -I${SRCDIR}/../../

#include "dependency_injector.h"
#include <stdlib.h>
#include <string.h>
*/
import "C"
import (
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"runtime"
	"unsafe"
)

// ErrorCode represents error codes from the library.
type ErrorCode int

const (
	// OK indicates the operation succeeded.
	OK ErrorCode = 0
	// NotFound indicates the service was not found.
	NotFound ErrorCode = 1
	// InvalidArgument indicates an invalid argument was provided.
	InvalidArgument ErrorCode = 2
	// AlreadyRegistered indicates the service is already registered.
	AlreadyRegistered ErrorCode = 3
	// InternalError indicates an internal error occurred.
	InternalError ErrorCode = 4
	// SerializationError indicates a serialization error occurred.
	SerializationError ErrorCode = 5
	// Locked indicates the container is locked and registration is not allowed.
	Locked ErrorCode = 6
)

func (e ErrorCode) Error() string {
	switch e {
	case OK:
		return "ok"
	case NotFound:
		return "service not found"
	case InvalidArgument:
		return "invalid argument"
	case AlreadyRegistered:
		return "service already registered"
	case InternalError:
		return "internal error"
	case SerializationError:
		return "serialization error"
	case Locked:
		return "container is locked - registration is not allowed"
	default:
		// Codes added by a newer native library must degrade gracefully
		// rather than panic or be silently reported as a known code.
		return fmt.Sprintf("unknown error code: %d", int(e))
	}
}

// DIError represents an error from the dependency injector.
type DIError struct {
	Code    ErrorCode
	Message string
}

func (e *DIError) Error() string {
	if e.Message != "" {
		return fmt.Sprintf("%s: %s", e.Code.Error(), e.Message)
	}
	return e.Code.Error()
}

// Is implements errors.Is interface for error checking.
func (e *DIError) Is(target error) bool {
	if t, ok := target.(*DIError); ok {
		return e.Code == t.Code
	}
	return false
}

// getLastError retrieves the last error message from the library.
func getLastError() string {
	cMsg := C.di_error_message()
	if cMsg == nil {
		return ""
	}
	defer C.di_string_free(cMsg)
	return C.GoString(cMsg)
}

// clearError clears the last error.
func clearError() {
	C.di_error_clear()
}

// Container wraps the Rust dependency injection container.
type Container struct {
	ptr *C.DiContainer
}

// NewContainer creates a new dependency injection container.
func NewContainer() *Container {
	ptr := C.di_container_new()
	if ptr == nil {
		return nil
	}

	c := &Container{ptr: ptr}
	runtime.SetFinalizer(c, (*Container).Free)
	return c
}

// Free releases the container resources.
// This is called automatically by the finalizer, but can be called explicitly.
// Safe to call on nil container.
func (c *Container) Free() {
	if c == nil {
		return
	}
	if c.ptr != nil {
		C.di_container_free(c.ptr)
		c.ptr = nil
	}
}

// Scope creates a child scope that inherits services from this container.
func (c *Container) Scope() (*Container, error) {
	if c.ptr == nil {
		return nil, errors.New("container is nil or freed")
	}

	clearError()
	ptr := C.di_container_scope(c.ptr)
	if ptr == nil {
		return nil, &DIError{
			Code:    InternalError,
			Message: getLastError(),
		}
	}

	child := &Container{ptr: ptr}
	runtime.SetFinalizer(child, (*Container).Free)
	return child, nil
}

// Register registers a singleton service with the given type name and data.
func (c *Container) Register(typeName string, data []byte) error {
	if c.ptr == nil {
		return errors.New("container is nil or freed")
	}

	clearError()
	cTypeName := C.CString(typeName)
	defer C.free(unsafe.Pointer(cTypeName))

	var dataPtr *C.uint8_t
	if len(data) > 0 {
		dataPtr = (*C.uint8_t)(unsafe.Pointer(&data[0]))
	}

	code := C.di_register_singleton(c.ptr, cTypeName, dataPtr, C.size_t(len(data)))
	if code != C.DI_OK {
		return &DIError{
			Code:    ErrorCode(code),
			Message: getLastError(),
		}
	}
	return nil
}

// RegisterJSON registers a singleton service with JSON data.
func (c *Container) RegisterJSON(typeName string, jsonData string) error {
	if c.ptr == nil {
		return errors.New("container is nil or freed")
	}

	clearError()
	cTypeName := C.CString(typeName)
	defer C.free(unsafe.Pointer(cTypeName))

	cJSON := C.CString(jsonData)
	defer C.free(unsafe.Pointer(cJSON))

	code := C.di_register_singleton_json(c.ptr, cTypeName, cJSON)
	if code != C.DI_OK {
		return &DIError{
			Code:    ErrorCode(code),
			Message: getLastError(),
		}
	}
	return nil
}

// RegisterValue registers a value by serializing it to JSON.
func (c *Container) RegisterValue(typeName string, value interface{}) error {
	data, err := json.Marshal(value)
	if err != nil {
		return fmt.Errorf("failed to marshal value: %w", err)
	}
	return c.Register(typeName, data)
}

// Resolve retrieves a service by type name and returns its raw JSON data.
// This uses the optimized di_resolve_json FFI function.
func (c *Container) Resolve(typeName string) ([]byte, error) {
	if c.ptr == nil {
		return nil, errors.New("container is nil or freed")
	}

	clearError()
	cTypeName := C.CString(typeName)
	defer C.free(unsafe.Pointer(cTypeName))

	// Use di_resolve_json for simpler and faster resolution
	jsonPtr := C.di_resolve_json(c.ptr, cTypeName)
	if jsonPtr == nil {
		errMsg := getLastError()
		if errMsg != "" {
			return nil, &DIError{
				Code:    NotFound,
				Message: errMsg,
			}
		}
		return nil, &DIError{
			Code:    NotFound,
			Message: fmt.Sprintf("service '%s' not found", typeName),
		}
	}
	defer C.di_string_free(jsonPtr)

	// Single copy into Go memory (GoString + []byte conversion would
	// allocate and copy twice). GoBytes takes a C.int length, so guard
	// against payloads that would overflow it.
	n := C.strlen(jsonPtr)
	if uint64(n) > uint64(math.MaxInt32) {
		return nil, fmt.Errorf("di: resolved service data too large (%d bytes)", uint64(n))
	}
	return C.GoBytes(unsafe.Pointer(jsonPtr), C.int(n)), nil
}

// ResolveInto retrieves a service and unmarshals it from JSON into the target.
func (c *Container) ResolveInto(typeName string, target interface{}) error {
	data, err := c.Resolve(typeName)
	if err != nil {
		return err
	}
	return json.Unmarshal(data, target)
}

// ResolveJSON is an alias for ResolveInto for backwards compatibility.
func (c *Container) ResolveJSON(typeName string, target interface{}) error {
	return c.ResolveInto(typeName, target)
}

// TryResolve attempts to resolve a service, returning nil if not found.
func (c *Container) TryResolve(typeName string) []byte {
	data, err := c.Resolve(typeName)
	if err != nil {
		return nil
	}
	return data
}

// Contains reports whether a service is registered under typeName.
//
// The underlying di_contains returns 1 when registered, 0 when not, and -1 on
// an internal error or invalid argument. The -1 case must not be collapsed
// into "not registered", so it is surfaced as a non-nil error and the boolean
// is meaningless in that case.
func (c *Container) Contains(typeName string) (bool, error) {
	if c == nil || c.ptr == nil {
		return false, errors.New("container is nil or freed")
	}

	clearError()
	cTypeName := C.CString(typeName)
	defer C.free(unsafe.Pointer(cTypeName))

	switch result := C.di_contains(c.ptr, cTypeName); result {
	case 1:
		return true, nil
	case 0:
		return false, nil
	default:
		return false, &DIError{
			Code:    InternalError,
			Message: getLastError(),
		}
	}
}

// Remove removes the service registered under typeName.
//
// It reports true when a service was removed and false when no service with
// that name was registered; neither case is an error. Removal is permitted on
// a locked container: locking blocks registration only.
func (c *Container) Remove(typeName string) (bool, error) {
	if c == nil || c.ptr == nil {
		return false, errors.New("container is nil or freed")
	}

	clearError()
	cTypeName := C.CString(typeName)
	defer C.free(unsafe.Pointer(cTypeName))

	switch code := ErrorCode(C.di_remove(c.ptr, cTypeName)); code {
	case OK:
		return true, nil
	case NotFound:
		return false, nil
	default:
		return false, &DIError{
			Code:    code,
			Message: getLastError(),
		}
	}
}

// Clear removes all registered services from the container.
//
// Clearing is permitted on a locked container: locking blocks registration
// only.
func (c *Container) Clear() error {
	if c == nil || c.ptr == nil {
		return errors.New("container is nil or freed")
	}

	clearError()
	if code := ErrorCode(C.di_clear(c.ptr)); code != OK {
		return &DIError{
			Code:    code,
			Message: getLastError(),
		}
	}
	return nil
}

// Lock locks the container so that no further services can be registered.
//
// Locking blocks registration only: after Lock, Register, RegisterJSON and
// RegisterValue fail with a *DIError whose Code is Locked, while Remove and
// Clear remain permitted. There is no unlock, and child scopes created with
// Scope start unlocked regardless of this container's lock state.
func (c *Container) Lock() error {
	if c == nil || c.ptr == nil {
		return errors.New("container is nil or freed")
	}

	clearError()
	C.di_lock(c.ptr)
	// di_lock returns void and reports failure (including a caught panic)
	// only through the thread-local last error.
	if msg := getLastError(); msg != "" {
		return &DIError{
			Code:    InternalError,
			Message: msg,
		}
	}
	return nil
}

// IsLocked reports whether the container has been locked with Lock.
//
// The underlying di_is_locked returns 1 when locked, 0 when not, and -1 on an
// internal error or invalid argument. The -1 case must not be collapsed into
// "not locked", so it is surfaced as a non-nil error and the boolean is
// meaningless in that case.
func (c *Container) IsLocked() (bool, error) {
	if c == nil || c.ptr == nil {
		return false, errors.New("container is nil or freed")
	}

	clearError()
	switch result := C.di_is_locked(c.ptr); result {
	case 1:
		return true, nil
	case 0:
		return false, nil
	default:
		return false, &DIError{
			Code:    InternalError,
			Message: getLastError(),
		}
	}
}

// ServiceCount returns the number of registered services.
//
// The native library returns -1 to signal an error (a null container or a
// panic caught at the FFI boundary); that sentinel is surfaced as an error
// rather than returned as a count.
func (c *Container) ServiceCount() (int64, error) {
	if c == nil || c.ptr == nil {
		return 0, &DIError{
			Code:    InvalidArgument,
			Message: "container is nil or freed",
		}
	}

	clearError()
	count := int64(C.di_service_count(c.ptr))
	if count < 0 {
		return 0, &DIError{
			Code:    InternalError,
			Message: getLastError(),
		}
	}
	return count, nil
}

// Version returns the library version.
func Version() string {
	return C.GoString(C.di_version())
}

// ErrNotFound is a sentinel error for not found services.
var ErrNotFound = &DIError{Code: NotFound}

// ErrAlreadyRegistered is a sentinel error for duplicate registrations.
var ErrAlreadyRegistered = &DIError{Code: AlreadyRegistered}

// ErrLocked is a sentinel error for registrations attempted on a locked
// container. Match it with errors.Is against the error returned by Register,
// RegisterJSON or RegisterValue.
var ErrLocked = &DIError{Code: Locked}

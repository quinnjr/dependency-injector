"""
Container implementation using ctypes FFI bindings.
"""

from __future__ import annotations

import ctypes
import json
import os
import sys
from ctypes import (
    POINTER,
    c_char_p,
    c_int,
    c_int32,
    c_int64,
    c_size_t,
    c_ubyte,
    c_void_p,
)
from enum import IntEnum
from pathlib import Path
from typing import Any, NoReturn, TypeVar

T = TypeVar("T")


class ErrorCode(IntEnum):
    """Error codes returned by the native library.

    Mirrors ``DiErrorCode`` in ``ffi/dependency_injector.h``.
    """

    OK = 0
    NOT_FOUND = 1
    INVALID_ARGUMENT = 2
    ALREADY_REGISTERED = 3
    INTERNAL_ERROR = 4
    SERIALIZATION_ERROR = 5
    LOCKED = 6


def _to_error_code(code: int) -> ErrorCode:
    """Convert a raw native error code into an :class:`ErrorCode`.

    ``ErrorCode(code)`` raises ``ValueError`` for an integer that is not a
    member, so a code added to the native ABI ahead of this binding would
    crash the error path instead of reporting the underlying failure.
    Unrecognized codes fall back to ``ErrorCode.INTERNAL_ERROR``.

    Args:
        code: The raw integer returned by the native library.

    Returns:
        The matching ``ErrorCode``, or ``ErrorCode.INTERNAL_ERROR`` if the
        code is not recognized by this binding.
    """
    try:
        return ErrorCode(code)
    except ValueError:
        return ErrorCode.INTERNAL_ERROR


class DIError(Exception):
    """Exception raised by the dependency injector."""

    def __init__(self, code: ErrorCode, message: str = ""):
        self.code = code
        self.message = message
        super().__init__(self._format_message())

    def _format_message(self) -> str:
        code_messages = {
            ErrorCode.OK: "Success",
            ErrorCode.NOT_FOUND: "Service not found",
            ErrorCode.INVALID_ARGUMENT: "Invalid argument",
            ErrorCode.ALREADY_REGISTERED: "Service already registered",
            ErrorCode.INTERNAL_ERROR: "Internal error",
            ErrorCode.SERIALIZATION_ERROR: "Serialization error",
            ErrorCode.LOCKED: "Container is locked",
        }
        base = code_messages.get(self.code, f"Unknown error code: {self.code}")
        if self.message:
            return f"{base}: {self.message}"
        return base


def _get_library_name() -> str:
    """Get the platform-specific library name."""
    if sys.platform == "win32":
        return "dependency_injector.dll"
    elif sys.platform == "darwin":
        return "libdependency_injector.dylib"
    else:
        return "libdependency_injector.so"


def _find_library() -> str:
    """Find the native library path.

    Search order:
    1. DI_LIBRARY_PATH environment variable
    2. Bundled native library (in package's native/ directory)
    3. Downloaded native library (in package's native/ directory)
    4. Local cargo build (target/release/)
    5. System paths
    """
    # Check environment variable first (highest priority)
    if env_path := os.environ.get("DI_LIBRARY_PATH"):
        if Path(env_path).exists():
            return env_path

    lib_name = _get_library_name()
    package_dir = Path(__file__).parent

    # Search paths in order of preference
    search_paths: list[Path | str] = [
        # 1. Bundled in package (from wheel with native library)
        package_dir / "native" / lib_name,

        # 2. Development: local cargo build
        package_dir.parent.parent.parent / "target" / "release" / lib_name,
        package_dir.parent.parent.parent.parent / "target" / "release" / lib_name,
        package_dir.parent.parent.parent.parent.parent / "target" / "release" / lib_name,

        # 3. System paths (Linux/macOS)
        Path("/usr/local/lib") / lib_name,
        Path("/usr/lib") / lib_name,

        # 4. Fallback to system library search
        lib_name,
    ]

    for path in search_paths:
        if isinstance(path, Path) and path.exists():
            return str(path)

    # Return system name and let ctypes try to find it
    return lib_name


def get_library_path() -> str:
    """Get the path to the loaded native library.

    Returns:
        The path to the native library that was loaded.
    """
    return _lib_path


# Load the native library
_lib_path = _find_library()
try:
    _lib = ctypes.CDLL(_lib_path)
except OSError as e:
    raise ImportError(
        f"Failed to load dependency-injector native library from '{_lib_path}'. "
        "Make sure you've built it with: cargo rustc --release --features ffi --crate-type cdylib\n"
        f"Original error: {e}"
    ) from e

# Define function signatures
_lib.di_container_new.argtypes = []
_lib.di_container_new.restype = c_void_p

_lib.di_container_free.argtypes = [c_void_p]
_lib.di_container_free.restype = None

_lib.di_container_scope.argtypes = [c_void_p]
_lib.di_container_scope.restype = c_void_p

_lib.di_register_singleton.argtypes = [c_void_p, c_char_p, POINTER(c_ubyte), c_size_t]
_lib.di_register_singleton.restype = c_int

_lib.di_register_singleton_json.argtypes = [c_void_p, c_char_p, c_char_p]
_lib.di_register_singleton_json.restype = c_int

# Returns a library-owned string that must be freed with di_string_free(),
# so the restype is c_void_p to keep the raw pointer (c_char_p would convert
# to bytes and discard the pointer, leaking the allocation).
_lib.di_resolve_json.argtypes = [c_void_p, c_char_p]
_lib.di_resolve_json.restype = c_void_p

_lib.di_contains.argtypes = [c_void_p, c_char_p]
_lib.di_contains.restype = c_int32

_lib.di_remove.argtypes = [c_void_p, c_char_p]
_lib.di_remove.restype = c_int

_lib.di_clear.argtypes = [c_void_p]
_lib.di_clear.restype = c_int

# di_lock returns void; failures are reported through di_error_message().
_lib.di_lock.argtypes = [c_void_p]
_lib.di_lock.restype = None

_lib.di_is_locked.argtypes = [c_void_p]
_lib.di_is_locked.restype = c_int32

_lib.di_service_count.argtypes = [c_void_p]
_lib.di_service_count.restype = c_int64

# Returns a library-owned string that must be freed with di_string_free().
_lib.di_error_message.argtypes = []
_lib.di_error_message.restype = c_void_p

_lib.di_error_clear.argtypes = []
_lib.di_error_clear.restype = None

_lib.di_string_free.argtypes = [c_void_p]
_lib.di_string_free.restype = None

_lib.di_version.argtypes = []
_lib.di_version.restype = c_char_p


def _take_native_string(ptr: int | None) -> str | None:
    """Decode a library-owned string and free it.

    The native library transfers ownership of strings returned from
    ``di_resolve_json`` and ``di_error_message``; they must be released
    with ``di_string_free``. Returns None for a NULL pointer.
    """
    if not ptr:
        return None
    try:
        return ctypes.string_at(ptr).decode("utf-8")
    finally:
        _lib.di_string_free(ptr)


def _get_last_error() -> str | None:
    """Get the last error message from the native library."""
    return _take_native_string(_lib.di_error_message())


def _clear_error() -> None:
    """Clear the last error message."""
    _lib.di_error_clear()


def _raise_native_error(code: int, default_message: str = "") -> NoReturn:
    """Raise a :class:`DIError` for a non-OK native return code.

    Consumes the thread-local message set by the native library and pairs it
    with the converted error code. A code this binding does not recognize is
    reported as ``ErrorCode.INTERNAL_ERROR`` with the raw value preserved in
    the message so a newer native ABI stays diagnosable.

    Args:
        code: The raw error code returned by the native library.
        default_message: Message to use when the library set no error text.

    Raises:
        DIError: Always.
    """
    error = _get_last_error()
    resolved = _to_error_code(code)
    message = error or default_message
    if resolved == ErrorCode.INTERNAL_ERROR and code != ErrorCode.INTERNAL_ERROR:
        detail = f"unrecognized native error code {code}"
        message = f"{detail}: {message}" if message else detail
    raise DIError(resolved, message)


class Container:
    """
    A high-performance dependency injection container.

    Services are stored by string type names and serialized as JSON.
    This allows seamless interop between Python objects and the Rust container.

    Example:
        >>> container = Container()
        >>>
        >>> # Register services
        >>> container.register("Config", {"debug": True, "port": 8080})
        >>> container.register("Database", {"host": "localhost", "port": 5432})
        >>>
        >>> # Resolve services
        >>> config = container.resolve("Config")
        >>> print(config["port"])  # 8080
        >>>
        >>> # Check existence
        >>> print(container.contains("Config"))  # True
        >>>
        >>> # Create scoped containers
        >>> request_scope = container.scope()
        >>> request_scope.register("RequestId", {"id": "req-123"})
        >>>
        >>> # Clean up
        >>> request_scope.free()
        >>> container.free()

    Note:
        Always call `free()` when done with the container, or use it as a
        context manager:

        >>> with Container() as container:
        ...     container.register("Service", {"data": "value"})
        ...     result = container.resolve("Service")
    """

    def __init__(self, _ptr: c_void_p | None = None):
        """
        Create a new dependency injection container.

        Args:
            _ptr: Internal use only. Native pointer for child scopes.
        """
        if _ptr is not None:
            self._ptr = _ptr
        else:
            self._ptr = _lib.di_container_new()
            if not self._ptr:
                raise DIError(ErrorCode.INTERNAL_ERROR, "Failed to create container")
        self._freed = False

    def __enter__(self) -> Container:
        """Context manager entry."""
        return self

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Context manager exit - automatically frees the container."""
        self.free()

    def __del__(self) -> None:
        """Destructor - frees the container if not already freed."""
        if hasattr(self, "_freed") and not self._freed:
            self.free()

    def _ensure_not_freed(self) -> None:
        """Raise an error if the container has been freed."""
        if self._freed or not self._ptr:
            raise DIError(ErrorCode.INVALID_ARGUMENT, "Container has been freed")

    def free(self) -> None:
        """
        Free the container and release native resources.

        After calling this method, the container can no longer be used.
        It's safe to call this method multiple times.
        """
        if not self._freed and self._ptr:
            _lib.di_container_free(self._ptr)
            self._freed = True
            self._ptr = None

    def scope(self) -> Container:
        """
        Create a child scope that inherits services from this container.

        Services registered in the child scope are not visible to the parent.
        The child scope can resolve services from the parent.

        Returns:
            A new scoped container.

        Example:
            >>> root = Container()
            >>> root.register("Config", {"env": "production"})
            >>>
            >>> request = root.scope()
            >>> request.register("User", {"id": 1})
            >>>
            >>> # Child can access parent's services
            >>> request.resolve("Config")  # Works!
            >>>
            >>> # Parent cannot access child's services
            >>> root.contains("User")  # False
            >>>
            >>> request.free()
            >>> root.free()
        """
        self._ensure_not_freed()
        _clear_error()
        child_ptr = _lib.di_container_scope(self._ptr)
        if not child_ptr:
            error = _get_last_error()
            raise DIError(ErrorCode.INTERNAL_ERROR, error or "Failed to create scope")
        return Container(_ptr=child_ptr)

    def register(self, type_name: str, value: Any) -> None:
        """
        Register a singleton service with the given type name.

        The value is serialized to JSON for storage in the native container.

        Args:
            type_name: A unique identifier for this service type.
            value: The service value (must be JSON-serializable).

        Raises:
            DIError: If the service is already registered, the container has
                been locked (``ErrorCode.LOCKED``), or serialization fails.

        Example:
            >>> container.register("Config", {"debug": True, "port": 8080})
            >>> container.register("Users", [{"id": 1, "name": "Alice"}])
        """
        self._ensure_not_freed()
        _clear_error()

        try:
            json_data = json.dumps(value)
        except (TypeError, ValueError) as e:
            raise DIError(
                ErrorCode.SERIALIZATION_ERROR, f"Failed to serialize value: {e}"
            ) from e

        type_name_bytes = type_name.encode("utf-8")
        json_bytes = json_data.encode("utf-8")

        code = _lib.di_register_singleton_json(self._ptr, type_name_bytes, json_bytes)
        if code != ErrorCode.OK:
            _raise_native_error(code, f"Failed to register '{type_name}'")

    def register_bytes(self, type_name: str, data: bytes) -> None:
        """
        Register a singleton service with raw bytes.

        Use this for binary data that shouldn't be JSON-serialized.

        Args:
            type_name: A unique identifier for this service type.
            data: The raw byte data.

        Raises:
            DIError: If the service is already registered or the container has
                been locked (``ErrorCode.LOCKED``).

        Example:
            >>> container.register_bytes("Blob", b"\\x00\\x01raw")
        """
        self._ensure_not_freed()
        _clear_error()

        type_name_bytes = type_name.encode("utf-8")
        data_array = (c_ubyte * len(data)).from_buffer_copy(data)

        code = _lib.di_register_singleton(
            self._ptr, type_name_bytes, data_array, len(data)
        )
        if code != ErrorCode.OK:
            _raise_native_error(code, f"Failed to register '{type_name}'")

    def resolve(self, type_name: str) -> Any:
        """
        Resolve a service by type name.

        The service data is deserialized from JSON.

        Args:
            type_name: The service type name to resolve.

        Returns:
            The deserialized service value.

        Raises:
            DIError: If the service is not found or deserialization fails.

        Example:
            >>> container.register("Config", {"debug": True, "port": 8080})
            >>> config = container.resolve("Config")
            >>> print(config["port"])  # 8080
        """
        self._ensure_not_freed()
        _clear_error()

        type_name_bytes = type_name.encode("utf-8")
        json_str = _take_native_string(_lib.di_resolve_json(self._ptr, type_name_bytes))

        if json_str is None:
            error = _get_last_error()
            if error:
                raise DIError(ErrorCode.NOT_FOUND, error)
            raise DIError(ErrorCode.NOT_FOUND, f"Service '{type_name}' not found")

        try:
            return json.loads(json_str)
        except json.JSONDecodeError as e:
            raise DIError(
                ErrorCode.SERIALIZATION_ERROR,
                f"Failed to deserialize service '{type_name}': {e}",
            ) from e

    def try_resolve(self, type_name: str) -> Any | None:
        """
        Try to resolve a service by type name.

        Unlike `resolve()`, this method returns None instead of raising
        an error if the service is not found.

        Args:
            type_name: The service type name to resolve.

        Returns:
            The deserialized service value, or None if not found.

        Example:
            >>> container.register("Config", {"debug": True})
            >>> config = container.try_resolve("Config")  # Returns dict
            >>> missing = container.try_resolve("Missing")  # Returns None
        """
        try:
            return self.resolve(type_name)
        except DIError as e:
            if e.code == ErrorCode.NOT_FOUND:
                return None
            raise

    def contains(self, type_name: str) -> bool:
        """
        Check if a service is registered.

        The native ``di_contains`` returns 1 for registered, 0 for not
        registered, and a negative value (-1) on error. Per the FFI contract
        in ``ffi/dependency_injector.h``, the negative case must not be
        collapsed into ``False`` - it signals an invalid argument or a caught
        internal panic, not an absent service - so it is raised instead.

        Args:
            type_name: The service type name to check.

        Returns:
            True if the service is registered, False if it is not.

        Raises:
            DIError: If the container has been freed, or the native library
                reported an error (negative return).

        Example:
            >>> container.register("Config", {"debug": True})
            >>> container.contains("Config")  # True
            >>> container.contains("Missing")  # False
        """
        self._ensure_not_freed()
        _clear_error()

        type_name_bytes = type_name.encode("utf-8")
        result = _lib.di_contains(self._ptr, type_name_bytes)
        if result < 0:
            self._raise_sentinel_error(
                f"di_contains failed for '{type_name}'",
            )
        return result == 1

    @staticmethod
    def _raise_sentinel_error(default_message: str) -> NoReturn:
        """Raise a DIError for a negative int sentinel (-1) return.

        The native library reports a message only for the caught-panic path;
        the invalid-argument paths return -1 with no message. Distinguish the
        two so the raised code is accurate.

        Args:
            default_message: Context used when the library set no error text.

        Raises:
            DIError: Always.
        """
        error = _get_last_error()
        if error:
            raise DIError(ErrorCode.INTERNAL_ERROR, error)
        raise DIError(ErrorCode.INVALID_ARGUMENT, default_message)

    def remove(self, type_name: str) -> bool:
        """
        Remove a registered service by type name.

        Removal is permitted on a locked container: locking blocks new
        registrations only, matching the core container's semantics.

        Args:
            type_name: The service type name to remove.

        Returns:
            True if the service was removed, False if no service with that
            name was registered.

        Raises:
            DIError: If the container has been freed, or the native library
                reported an error other than "not found".

        Example:
            >>> container.register("Config", {"debug": True})
            >>> container.remove("Config")  # True
            >>> container.remove("Config")  # False (already gone)
        """
        self._ensure_not_freed()
        _clear_error()

        type_name_bytes = type_name.encode("utf-8")
        code = _lib.di_remove(self._ptr, type_name_bytes)
        if code == ErrorCode.OK:
            return True
        if code == ErrorCode.NOT_FOUND:
            # The library sets an error message for NotFound; drain it so it
            # cannot be misattributed to a later call.
            _clear_error()
            return False
        _raise_native_error(code, f"Failed to remove '{type_name}'")

    def clear(self) -> None:
        """
        Remove all registered services from this container.

        Clearing is permitted on a locked container: locking blocks new
        registrations only, matching the core container's semantics.

        Raises:
            DIError: If the container has been freed or the native library
                reported an error.

        Example:
            >>> container.register("A", {"id": 1})
            >>> container.register("B", {"id": 2})
            >>> container.clear()
            >>> container.service_count  # 0
        """
        self._ensure_not_freed()
        _clear_error()

        code = _lib.di_clear(self._ptr)
        if code != ErrorCode.OK:
            _raise_native_error(code, "Failed to clear container")

    def lock(self) -> None:
        """
        Lock this container to prevent further registrations.

        Locking blocks registration only: `remove()` and `clear()` remain
        permitted on a locked container, matching the core container's
        semantics. There is no unlock. Child scopes created with `scope()`
        start unlocked regardless of this container's lock state.

        After locking, `register()` and `register_bytes()` raise a `DIError`
        carrying `ErrorCode.LOCKED`.

        Raises:
            DIError: If the container has been freed or the native library
                reported an error.

        Example:
            >>> container.register("Config", {"debug": True})
            >>> container.lock()
            >>> container.is_locked()  # True
            >>> container.register("Late", {})  # raises DIError (LOCKED)
            >>> container.remove("Config")  # True - removal still allowed
        """
        self._ensure_not_freed()
        _clear_error()

        # di_lock returns void; a failure is reported only via the
        # thread-local error message.
        _lib.di_lock(self._ptr)
        error = _get_last_error()
        if error:
            raise DIError(ErrorCode.INVALID_ARGUMENT, error)

    def is_locked(self) -> bool:
        """
        Check whether this container is locked.

        The native ``di_is_locked`` returns 1 for locked, 0 for unlocked, and
        a negative value (-1) on error. As with `contains()`, the negative
        case is an error and must not be collapsed into ``False``, so it is
        raised.

        Returns:
            True if the container is locked, False if it is not.

        Raises:
            DIError: If the container has been freed, or the native library
                reported an error (negative return).

        Example:
            >>> container.is_locked()  # False
            >>> container.lock()
            >>> container.is_locked()  # True
        """
        self._ensure_not_freed()
        _clear_error()

        result = _lib.di_is_locked(self._ptr)
        if result < 0:
            self._raise_sentinel_error("di_is_locked failed")
        return result == 1

    @property
    def service_count(self) -> int:
        """
        Get the number of registered services.

        The native library returns -1 to signal an error (a panic caught at
        the FFI boundary); that sentinel is raised rather than returned as a
        count, so this never reports a negative number of services.

        Returns:
            The number of services in the container.

        Raises:
            DIError: If the native call fails.
        """
        self._ensure_not_freed()
        _clear_error()
        count = int(_lib.di_service_count(self._ptr))
        if count < 0:
            self._raise_sentinel_error(
                f"di_service_count failed (native returned {count})",
            )
        return count

    @staticmethod
    def version() -> str:
        """
        Get the library version.

        Returns:
            The version string.
        """
        return _lib.di_version().decode("utf-8")

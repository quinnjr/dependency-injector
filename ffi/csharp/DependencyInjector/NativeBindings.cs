using System;
using System.Runtime.InteropServices;

namespace DependencyInjector.Native
{
    /// <summary>
    /// Error codes returned by the native library.
    /// </summary>
    public enum DiErrorCode : int
    {
        /// <summary>Operation succeeded.</summary>
        Ok = 0,
        /// <summary>Service not found.</summary>
        NotFound = 1,
        /// <summary>Invalid argument (null pointer, invalid UTF-8, etc.).</summary>
        InvalidArgument = 2,
        /// <summary>Service already registered.</summary>
        AlreadyRegistered = 3,
        /// <summary>Internal error.</summary>
        InternalError = 4,
        /// <summary>Serialization/deserialization error.</summary>
        SerializationError = 5,
        /// <summary>Container is locked - registration is not allowed.</summary>
        Locked = 6,
    }

    /// <summary>
    /// Result type for resolve operations.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    internal struct DiResult
    {
        public DiErrorCode Code;
        public IntPtr Service;
    }

    /// <summary>
    /// Native P/Invoke bindings to the Rust dependency-injector library.
    /// </summary>
    internal static class NativeBindings
    {
        private const string LibraryName = "dependency_injector";

        // ============================================================================
        // Container Lifecycle
        // ============================================================================

        /// <summary>
        /// Create a new dependency injection container.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr di_container_new();

        /// <summary>
        /// Free a container and all its resources.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern void di_container_free(IntPtr container);

        /// <summary>
        /// Create a child scope from a container.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr di_container_scope(IntPtr container);

        // ============================================================================
        // Service Registration
        // ============================================================================

        /// <summary>
        /// Register a singleton service with raw byte data.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern DiErrorCode di_register_singleton(
            IntPtr container,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string typeName,
            byte[] data,
            UIntPtr dataLen);

        /// <summary>
        /// Register a singleton service with a JSON string.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern DiErrorCode di_register_singleton_json(
            IntPtr container,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string typeName,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string jsonData);

        // ============================================================================
        // Service Removal and Locking
        // ============================================================================

        /// <summary>
        /// Remove a registered service by type name.
        /// </summary>
        /// <remarks>
        /// Returns <see cref="DiErrorCode.Ok"/> if the service was removed,
        /// <see cref="DiErrorCode.NotFound"/> if no service with that name is
        /// registered, or <see cref="DiErrorCode.InvalidArgument"/> for a null
        /// container or an invalid type name. Removal is permitted on a locked
        /// container.
        /// </remarks>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern DiErrorCode di_remove(
            IntPtr container,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string typeName);

        /// <summary>
        /// Remove all registered services from a container.
        /// </summary>
        /// <remarks>
        /// Returns <see cref="DiErrorCode.Ok"/> on success, or
        /// <see cref="DiErrorCode.InvalidArgument"/> if the container is null.
        /// Clearing is permitted on a locked container.
        /// </remarks>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern DiErrorCode di_clear(IntPtr container);

        /// <summary>
        /// Lock a container to prevent further registrations.
        /// </summary>
        /// <remarks>
        /// Once locked, the registration entry points return
        /// <see cref="DiErrorCode.Locked"/>. Removal and clearing remain
        /// permitted. There is no unlock.
        /// </remarks>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern void di_lock(IntPtr container);

        /// <summary>
        /// Check whether a container is locked.
        /// </summary>
        /// <remarks>
        /// Returns 1 if locked, 0 if not, and -1 on error. A negative return
        /// signals an internal error or invalid argument (see
        /// <see cref="di_error_message"/>) and must not be collapsed into
        /// "false".
        /// </remarks>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern int di_is_locked(IntPtr container);

        // ============================================================================
        // Service Resolution
        // ============================================================================

        /// <summary>
        /// Resolve a service by type name (returns DiResult struct).
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern DiResult di_resolve(
            IntPtr container,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string typeName);

        /// <summary>
        /// Resolve a service and return its data as a JSON string.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr di_resolve_json(
            IntPtr container,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string typeName);

        /// <summary>
        /// Check if a service is registered.
        /// </summary>
        /// <remarks>
        /// Returns 1 if registered, 0 if not, and -1 on error. A negative
        /// return signals an internal error or invalid argument (see
        /// <see cref="di_error_message"/>) and must not be collapsed into
        /// "false".
        /// </remarks>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern int di_contains(
            IntPtr container,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string typeName);

        // ============================================================================
        // Service Data Access
        // ============================================================================

        /// <summary>
        /// Get the data pointer from a service handle.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr di_service_data(IntPtr service);

        /// <summary>
        /// Get the data length from a service handle.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern UIntPtr di_service_data_len(IntPtr service);

        /// <summary>
        /// Get the type name from a service handle.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr di_service_type_name(IntPtr service);

        /// <summary>
        /// Free a service handle.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern void di_service_free(IntPtr service);

        // ============================================================================
        // Error Handling
        // ============================================================================

        /// <summary>
        /// Get the last error message (thread-local).
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr di_error_message();

        /// <summary>
        /// Clear the last error message.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern void di_error_clear();

        /// <summary>
        /// Free a string returned by the library.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern void di_string_free(IntPtr str);

        // ============================================================================
        // Utility Functions
        // ============================================================================

        /// <summary>
        /// Get the library version.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr di_version();

        /// <summary>
        /// Get the number of registered services in a container.
        /// </summary>
        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
        public static extern long di_service_count(IntPtr container);
    }
}

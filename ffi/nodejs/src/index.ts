/**
 * Node.js bindings for the dependency-injector Rust library.
 *
 * This module provides a high-level TypeScript API for the dependency injection
 * container, wrapping the native FFI calls using koffi.
 *
 * @example
 * ```typescript
 * import { Container } from '@pegasusheavy/dependency-injector';
 *
 * const container = new Container();
 *
 * // Register a service
 * container.register('Config', { debug: true, port: 8080 });
 *
 * // Resolve the service
 * const config = container.resolve<{ debug: boolean; port: number }>('Config');
 * console.log(config.port); // 8080
 *
 * container.free();
 * ```
 *
 * @module
 */

import koffi from "koffi";
import path from "path";
import { fileURLToPath } from "url";
import fs from "fs";

/**
 * Error codes from the native library.
 */
export enum ErrorCode {
  Ok = 0,
  NotFound = 1,
  InvalidArgument = 2,
  AlreadyRegistered = 3,
  InternalError = 4,
  SerializationError = 5,
}

/**
 * Error thrown by the dependency injector.
 */
export class DIError extends Error {
  constructor(
    public readonly code: ErrorCode,
    message: string,
    cause?: unknown
  ) {
    super(message, cause === undefined ? undefined : { cause });
    this.name = "DIError";
  }

  static fromCode(code: ErrorCode, lastError?: string): DIError {
    const messages: Record<ErrorCode, string> = {
      [ErrorCode.Ok]: "Success",
      [ErrorCode.NotFound]: "Service not found",
      [ErrorCode.InvalidArgument]: "Invalid argument",
      [ErrorCode.AlreadyRegistered]: "Service already registered",
      [ErrorCode.InternalError]: "Internal error",
      [ErrorCode.SerializationError]: "Serialization error",
    };
    const baseMessage = messages[code] || `Unknown error code: ${code}`;
    const fullMessage = lastError ? `${baseMessage}: ${lastError}` : baseMessage;
    return new DIError(code, fullMessage);
  }
}

/**
 * Platform-specific library names.
 */
const LIBRARY_NAMES: Record<string, string> = {
  linux: "libdependency_injector.so",
  darwin: "libdependency_injector.dylib",
  win32: "dependency_injector.dll",
};

/**
 * Find the native library path.
 */
function findLibraryPath(): string {
  // Get current file directory (ESM compatible)
  const __filename = fileURLToPath(import.meta.url);
  const __dirname = path.dirname(__filename);

  const libName = LIBRARY_NAMES[process.platform];
  if (!libName) {
    throw new Error(`Unsupported platform: ${process.platform}`);
  }

  // Try multiple locations in order of preference
  const possiblePaths = [
    // 1. Custom path from environment (highest priority)
    process.env.DI_LIBRARY_PATH,

    // 2. Downloaded pre-built library (from postinstall)
    path.resolve(__dirname, "../native", libName),
    path.resolve(__dirname, "../../native", libName),

    // 3. Development: local cargo build
    path.resolve(__dirname, "../../../target/release", libName),
    path.resolve(__dirname, "../../../../target/release", libName),
    path.resolve(__dirname, "../../../../../target/release", libName),

    // 4. System paths (Linux/macOS)
    `/usr/local/lib/${libName}`,
    `/usr/lib/${libName}`,
  ].filter(Boolean) as string[];

  // Find first existing path
  for (const p of possiblePaths) {
    try {
      if (fs.existsSync(p)) {
        return p;
      }
    } catch {
      // Continue to next path
    }
  }

  // Return helpful error message
  throw new Error(
    `Native library not found. Searched:\n` +
    possiblePaths.map(p => `  - ${p}`).join('\n') +
    `\n\nTo fix this:\n` +
    `  1. Run: cargo rustc --release --features ffi --crate-type cdylib\n` +
    `  2. Or set DI_LIBRARY_PATH environment variable\n` +
    `  3. Or reinstall the package to download pre-built binaries`
  );
}

// Define koffi types
const ContainerPtr = koffi.pointer("DiContainer", koffi.opaque());
const ServicePtr = koffi.pointer("DiService", koffi.opaque());
// Raw pointer to a Rust-allocated, NUL-terminated string. Strings returned
// through this type are owned by the native library and MUST be released
// with di_string_free() after decoding.
const CharPtr = koffi.pointer("char");

// Load the native library
let lib: ReturnType<typeof koffi.load>;
let libraryPath: string;

try {
  libraryPath = findLibraryPath();
  lib = koffi.load(libraryPath);
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  throw new Error(
    `Failed to load dependency-injector native library.\n\n${message}`
  );
}

// Define FFI functions
const di_container_new = lib.func("di_container_new", ContainerPtr, []);
const di_container_free = lib.func("di_container_free", "void", [ContainerPtr]);
const di_container_scope = lib.func("di_container_scope", ContainerPtr, [ContainerPtr]);

const di_register_singleton = lib.func("di_register_singleton", "int", [
  ContainerPtr,
  "str",
  koffi.pointer("uint8_t"),
  "size_t",
]);
const di_register_singleton_json = lib.func("di_register_singleton_json", "int", [
  ContainerPtr,
  "str",
  "str",
]);

const di_resolve_json = lib.func("di_resolve_json", CharPtr, [ContainerPtr, "str"]);
const di_contains = lib.func("di_contains", "int", [ContainerPtr, "str"]);
const di_service_count = lib.func("di_service_count", "int64", [ContainerPtr]);

const di_error_message = lib.func("di_error_message", CharPtr, []);
const di_error_clear = lib.func("di_error_clear", "void", []);
const di_string_free = lib.func("di_string_free", "void", [CharPtr]);

const di_version = lib.func("di_version", "str", []);

/**
 * Decode a library-owned C string and free the native allocation.
 *
 * The pointer comes from a native function that allocates with
 * `CString::into_raw` (see ffi/dependency_injector.h); it must be released
 * with `di_string_free()` exactly once. The free happens in a `finally`
 * block so it runs even if decoding throws.
 *
 * @param ptr - Pointer returned by the native library, or null.
 * @returns The decoded string, or `null` if the pointer was null.
 */
function takeNativeString(ptr: unknown): string | null {
  if (!ptr) {
    return null;
  }
  try {
    return koffi.decode(ptr, "char", -1) as string;
  } finally {
    di_string_free(ptr);
  }
}

/**
 * Get the last error message from the native library.
 */
function getLastError(): string | null {
  const error = takeNativeString(di_error_message());
  if (!error) {
    return null;
  }
  return error;
}

/**
 * Clear the last error in the native library.
 */
function clearError(): void {
  di_error_clear();
}

/**
 * A dependency injection container.
 *
 * The container stores services by string type names and serializes them as JSON.
 * This allows seamless interop between TypeScript objects and the Rust container.
 *
 * @example
 * ```typescript
 * const container = new Container();
 *
 * // Register services
 * container.register('Database', { host: 'localhost', port: 5432 });
 * container.register('Config', { debug: true });
 *
 * // Resolve services
 * const db = container.resolve<{ host: string; port: number }>('Database');
 *
 * // Create scoped containers
 * const requestScope = container.scope();
 * requestScope.register('RequestId', { id: 'req-123' });
 *
 * requestScope.free();
 * container.free();
 * ```
 */
export class Container {
  private ptr: unknown | null;
  private isFreed = false;

  /**
   * Create a new dependency injection container.
   */
  constructor() {
    this.ptr = di_container_new();
    if (!this.ptr) {
      throw new DIError(ErrorCode.InternalError, "Failed to create container");
    }
  }

  /**
   * Create a container from an existing native pointer.
   * @internal
   */
  private static fromPtr(ptr: unknown): Container {
    const container = Object.create(Container.prototype);
    container.ptr = ptr;
    container.isFreed = false;
    return container;
  }

  /**
   * Check if the container has been freed.
   */
  private ensureNotFreed(): void {
    if (this.isFreed || !this.ptr) {
      throw new DIError(ErrorCode.InvalidArgument, "Container has been freed");
    }
  }

  /**
   * Free the container and release native resources.
   *
   * After calling this method, the container can no longer be used.
   */
  free(): void {
    if (!this.isFreed && this.ptr) {
      di_container_free(this.ptr);
      this.isFreed = true;
      this.ptr = null;
    }
  }

  /**
   * Create a child scope that inherits services from this container.
   *
   * Services registered in the child scope are not visible to the parent.
   * The child scope can resolve services from the parent.
   *
   * @returns A new scoped container.
   *
   * @example
   * ```typescript
   * const root = new Container();
   * root.register('Config', { env: 'production' });
   *
   * const request = root.scope();
   * request.register('User', { id: 1 });
   *
   * // Child can access parent's services
   * request.resolve('Config'); // Works
   *
   * // Parent cannot access child's services
   * root.contains('User'); // false
   *
   * request.free();
   * root.free();
   * ```
   */
  scope(): Container {
    this.ensureNotFreed();
    clearError();
    const childPtr = di_container_scope(this.ptr!);
    if (!childPtr) {
      const error = getLastError();
      throw new DIError(ErrorCode.InternalError, error || "Failed to create scope");
    }
    return Container.fromPtr(childPtr);
  }

  /**
   * Register a singleton service with the given type name.
   *
   * The value is serialized to JSON for storage in the native container.
   *
   * @param typeName - A unique identifier for this service type.
   * @param value - The service value (must be JSON-serializable).
   * @throws {DIError} If the service is already registered or serialization fails.
   *
   * @example
   * ```typescript
   * container.register('Config', { debug: true, port: 8080 });
   * container.register('Database', { host: 'localhost' });
   * ```
   */
  register<T>(typeName: string, value: T): void {
    this.ensureNotFreed();
    clearError();

    let json: string;
    try {
      json = JSON.stringify(value);
    } catch (error) {
      throw new DIError(
        ErrorCode.SerializationError,
        `Failed to serialize value: ${error}`
      );
    }

    const code = di_register_singleton_json(this.ptr!, typeName, json);
    if (code !== ErrorCode.Ok) {
      const error = getLastError();
      throw DIError.fromCode(code, error || undefined);
    }
  }

  /**
   * Resolve a service by type name.
   *
   * The service data is deserialized from JSON.
   *
   * @param typeName - The service type name to resolve.
   * @returns The deserialized service value.
   * @throws {DIError} If the service is not found or deserialization fails.
   *
   * @example
   * ```typescript
   * interface Config {
   *   debug: boolean;
   *   port: number;
   * }
   *
   * container.register('Config', { debug: true, port: 8080 });
   * const config = container.resolve<Config>('Config');
   * console.log(config.port); // 8080
   * ```
   */
  resolve<T>(typeName: string): T {
    this.ensureNotFreed();
    clearError();

    const json = takeNativeString(di_resolve_json(this.ptr!, typeName));
    // Only a null pointer means "not found" — an empty string is a
    // successfully resolved (if unparseable) value and falls through to
    // JSON.parse, which reports it as a serialization error. An empty
    // native string is unreachable via this binding's register() (which
    // always produces non-empty JSON); the branch is defensive for
    // containers populated directly through the C ABI.
    if (json === null) {
      const error = getLastError();
      if (error) {
        throw new DIError(ErrorCode.NotFound, error);
      }
      throw new DIError(ErrorCode.NotFound, `Service '${typeName}' not found`);
    }

    try {
      return JSON.parse(json) as T;
    } catch (error) {
      throw new DIError(
        ErrorCode.SerializationError,
        `Failed to deserialize service '${typeName}': ${error}`,
        error
      );
    }
  }

  /**
   * Check if a service is registered.
   *
   * @param typeName - The service type name to check.
   * @returns `true` if the service is registered, `false` otherwise.
   */
  contains(typeName: string): boolean {
    this.ensureNotFreed();
    const result = di_contains(this.ptr!, typeName);
    return result === 1;
  }

  /**
   * Get the number of registered services.
   *
   * @returns The number of services in the container.
   */
  get serviceCount(): number {
    this.ensureNotFreed();
    return Number(di_service_count(this.ptr!));
  }

  /**
   * Get the library version.
   *
   * @returns The version string.
   */
  static version(): string {
    return di_version();
  }

  /**
   * Get the path to the loaded native library.
   *
   * @returns The absolute path to the native library.
   */
  static libraryPath(): string {
    return libraryPath;
  }
}

// Re-export types
export { ErrorCode as DiErrorCode };
export default Container;

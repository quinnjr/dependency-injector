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
  /** Container is locked - registration is not allowed. */
  Locked = 6,
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
    // Declared as an exhaustive `Record<ErrorCode, string>` so that adding a
    // member to ErrorCode without a message is a compile error.
    const messages: Record<ErrorCode, string> = {
      [ErrorCode.Ok]: "Success",
      [ErrorCode.NotFound]: "Service not found",
      [ErrorCode.InvalidArgument]: "Invalid argument",
      [ErrorCode.AlreadyRegistered]: "Service already registered",
      [ErrorCode.InternalError]: "Internal error",
      [ErrorCode.SerializationError]: "Serialization error",
      [ErrorCode.Locked]: "Container is locked - registration is not allowed",
    };
    // `code` arrives from the native library as a raw number and may be a
    // value this build's ErrorCode does not know about (a newer library adding
    // an error code). Indexing the exhaustive record would tell TypeScript the
    // result is always a string, so the lookup is widened to admit the
    // undefined that actually occurs and the fallback message covers it.
    const baseMessage =
      (messages as Record<number, string | undefined>)[code] ??
      `Unknown error code: ${code}`;
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
//
// This is deliberately an *opaque* pointer type rather than
// `koffi.pointer("char")`: koffi auto-decodes `char*` return values into JS
// strings (without transferring ownership), which would leak the native
// allocation and make it impossible to pass the original pointer back to
// `di_string_free()`. Freeing the auto-decoded JS string instead makes koffi
// marshal it into a temporary buffer that Rust's `CString::from_raw` then
// frees, corrupting the heap (SIGSEGV). With an opaque type koffi returns the
// raw external pointer, `koffi.decode(ptr, "char", -1)` reads the string, and
// `di_string_free()` receives the exact pointer Rust allocated. It also makes
// passing a plain JS string to `di_string_free()` a type error.
const RawCharPtr = koffi.pointer("RawChar", koffi.opaque());

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

const di_remove = lib.func("di_remove", "int", [ContainerPtr, "str"]);
const di_clear = lib.func("di_clear", "int", [ContainerPtr]);
const di_lock = lib.func("di_lock", "void", [ContainerPtr]);
const di_is_locked = lib.func("di_is_locked", "int", [ContainerPtr]);

const di_resolve_json = lib.func("di_resolve_json", RawCharPtr, [ContainerPtr, "str"]);
const di_contains = lib.func("di_contains", "int", [ContainerPtr, "str"]);
const di_service_count = lib.func("di_service_count", "int64", [ContainerPtr]);

const di_error_message = lib.func("di_error_message", RawCharPtr, []);
const di_error_clear = lib.func("di_error_clear", "void", []);
const di_string_free = lib.func("di_string_free", "void", [RawCharPtr]);

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
 * Decode a tri-state `int32_t` sentinel returned by the native library.
 *
 * `di_contains()` and `di_is_locked()` return 1 for true, 0 for false and a
 * negative value for "an error occurred - consult di_error_message()". Per
 * ffi/dependency_injector.h callers must NOT collapse the negative case into
 * `false`: doing so reports an internal error or invalid argument as a
 * confident "no". It is surfaced as a thrown DIError instead.
 *
 * Callers must call `clearError()` before the native call so that the message
 * read here belongs to this operation.
 *
 * @param result - Raw value returned by the native function.
 * @param operation - Description of the call, used when no native message is set.
 * @returns `true` for 1, `false` for 0.
 * @throws {DIError} If `result` is negative.
 */
function decodeTriState(result: number, operation: string): boolean {
  if (result === 1) {
    return true;
  }
  if (result === 0) {
    return false;
  }
  const error = getLastError();
  // The container pointer is non-null (ensureNotFreed) and koffi always hands
  // the native side a valid NUL-terminated string, so the remaining cause is a
  // panic caught at the FFI boundary: an internal error.
  throw DIError.fromCode(
    ErrorCode.InternalError,
    error || `${operation} failed (native returned ${result})`
  );
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
   * Inheritance is a snapshot taken at creation time, and the child starts
   * unlocked regardless of this container's lock state.
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
   * @throws {DIError} If the service is already registered
   *   (`ErrorCode.AlreadyRegistered`), the container has been locked with
   *   {@link Container.lock} (`ErrorCode.Locked`), or serialization fails
   *   (`ErrorCode.SerializationError`).
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
   * A native failure is reported as a thrown error rather than `false`: the
   * C ABI returns -1 for "an error occurred" and collapsing that into "not
   * registered" would hide it (see ffi/dependency_injector.h).
   *
   * @param typeName - The service type name to check.
   * @returns `true` if the service is registered, `false` if it is not.
   * @throws {DIError} If the native call reports an error (-1), or if the
   *   container has been freed.
   *
   * @example
   * ```typescript
   * container.register('Config', { debug: true });
   * container.contains('Config');  // true
   * container.contains('Missing'); // false
   * ```
   */
  contains(typeName: string): boolean {
    this.ensureNotFreed();
    clearError();
    return decodeTriState(
      di_contains(this.ptr!, typeName),
      `contains('${typeName}')`
    );
  }

  /**
   * Remove a registered service by type name.
   *
   * Removal is permitted on a locked container: locking blocks new
   * registrations only.
   *
   * @param typeName - The service type name to remove.
   * @returns `true` if the service was removed, `false` if no service with
   *   that name was registered.
   * @throws {DIError} If the native call fails for any reason other than the
   *   service being absent.
   *
   * @example
   * ```typescript
   * container.register('Cache', { ttl: 60 });
   * container.remove('Cache');   // true
   * container.contains('Cache'); // false
   * container.remove('Cache');   // false (already gone)
   * ```
   */
  remove(typeName: string): boolean {
    this.ensureNotFreed();
    clearError();

    const code = di_remove(this.ptr!, typeName);
    if (code === ErrorCode.Ok) {
      return true;
    }
    // NotFound is an expected outcome, not a failure: the native side sets an
    // error message for it, which the next clearError() discards.
    if (code === ErrorCode.NotFound) {
      return false;
    }
    const error = getLastError();
    throw DIError.fromCode(code, error || undefined);
  }

  /**
   * Remove all registered services from this container.
   *
   * Clearing is permitted on a locked container: locking blocks new
   * registrations only. Child scopes already created keep their own snapshot
   * of the services and are unaffected.
   *
   * @throws {DIError} If the native call fails.
   *
   * @example
   * ```typescript
   * container.register('A', { id: 1 });
   * container.register('B', { id: 2 });
   * container.clear();
   * console.log(container.serviceCount); // 0
   * ```
   */
  clear(): void {
    this.ensureNotFreed();
    clearError();

    const code = di_clear(this.ptr!);
    if (code !== ErrorCode.Ok) {
      const error = getLastError();
      throw DIError.fromCode(code, error || undefined);
    }
  }

  /**
   * Lock this container to prevent further registrations.
   *
   * Locking blocks registration only: {@link Container.remove} and
   * {@link Container.clear} remain permitted, and resolution is unaffected.
   * After locking, {@link Container.register} throws a {@link DIError} with
   * `code === ErrorCode.Locked`. There is no unlock. Child scopes created with
   * {@link Container.scope} start unlocked regardless of this container's
   * lock state.
   *
   * @throws {DIError} If the native call fails.
   *
   * @example
   * ```typescript
   * container.register('Config', { env: 'production' });
   * container.lock();
   *
   * container.isLocked(); // true
   * container.register('Late', {}); // throws DIError (ErrorCode.Locked)
   * container.remove('Config');     // still allowed -> true
   * ```
   */
  lock(): void {
    this.ensureNotFreed();
    clearError();

    di_lock(this.ptr!);
    // di_lock() returns void. Its only documented failure is a null container,
    // which ensureNotFreed() has already excluded, so a message here means a
    // panic was caught at the FFI boundary - surface it instead of silently
    // leaving the container unlocked.
    const error = getLastError();
    if (error) {
      throw new DIError(ErrorCode.InternalError, `Failed to lock container: ${error}`);
    }
  }

  /**
   * Check whether this container is locked.
   *
   * A native failure is reported as a thrown error rather than `false`: the
   * C ABI returns -1 for "an error occurred" and collapsing that into
   * "not locked" would hide it (see ffi/dependency_injector.h).
   *
   * @returns `true` if the container is locked, `false` if it is not.
   * @throws {DIError} If the native call reports an error (-1), or if the
   *   container has been freed.
   *
   * @example
   * ```typescript
   * container.isLocked(); // false
   * container.lock();
   * container.isLocked(); // true
   * ```
   */
  isLocked(): boolean {
    this.ensureNotFreed();
    clearError();
    return decodeTriState(di_is_locked(this.ptr!), "isLocked()");
  }

  /**
   * Get the number of registered services.
   *
   * The native library returns -1 to signal an error (a panic caught at the
   * FFI boundary); that sentinel is thrown rather than returned as a count,
   * so this getter never reports a negative number of services.
   *
   * @returns The number of services in the container.
   * @throws {DIError} If the native call fails.
   */
  get serviceCount(): number {
    this.ensureNotFreed();
    clearError();
    const count = Number(di_service_count(this.ptr!));
    if (count < 0) {
      throw DIError.fromCode(
        ErrorCode.InternalError,
        getLastError() || `serviceCount failed (native returned ${count})`
      );
    }
    return count;
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

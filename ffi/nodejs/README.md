# dependency-injector

Node.js/TypeScript bindings for the high-performance Rust dependency injection container.

## Features

- 🚀 **High Performance** - Native Rust implementation with ~10ns resolution
- 📦 **Type-Safe** - Full TypeScript support with generics
- 🔄 **Scoped Containers** - Hierarchical scopes for request-level isolation
- 🧵 **Thread-Safe** - Safe to use in worker threads
- 🔌 **FFI-Based** - Direct native bindings via koffi (no native compilation required)
- ⚡ **SWC-Powered** - Lightning-fast builds with SWC
- 📥 **Pre-built Binaries** - Automatic download of pre-built native libraries

## Installation

```bash
pnpm add dependency-injector
```

The package automatically downloads pre-built native libraries for:

| Platform | Architecture |
|----------|--------------|
| Linux | x64, arm64 |
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Windows | x64 |

### Manual Build (Optional)

If pre-built binaries aren't available for your platform, or you want to build from source:

```bash
# Install Rust if needed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/quinnjr/dependency-injector
cd dependency-injector
cargo rustc --release --features ffi --crate-type cdylib

# Point to your build (optional - package will auto-detect)
export DI_LIBRARY_PATH=$(pwd)/target/release/libdependency_injector.so
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `DI_LIBRARY_PATH` | Custom path to native library |
| `DI_SKIP_DOWNLOAD` | Skip automatic download (for CI/offline) |
| `DI_GITHUB_TOKEN` | GitHub token for rate limiting and private-repo access. When set, assets are fetched through the `api.github.com` asset endpoint so the token authenticates the download; it is re-evaluated per redirect hop and is never sent to download hosts (e.g. `objects.githubusercontent.com`) |
| `DI_REQUIRE_CHECKSUM` | When set (non-empty), a release with no `SHA256SUMS` asset is a hard failure (exit 1) instead of a warning |

### Install Failure Policy

The postinstall script distinguishes availability problems (soft) from
integrity problems (hard):

| Situation | Behaviour |
|-----------|-----------|
| Release metadata fetch fails, platform asset missing from the release, or the download fails | Warn, print build-from-source instructions, **exit 0** so the install completes |
| Checksum mismatch, `SHA256SUMS` present but with no entry for the asset, or an existing `SHA256SUMS` asset cannot be fetched | **Exit 1**; the downloaded file is deleted |
| Release has no `SHA256SUMS` asset at all (pre-checksum release) | Warn and proceed — unless `DI_REQUIRE_CHECKSUM` is set, then **exit 1** |
| Unsupported platform | **Exit 1** |

Downloads are staged at `<output>.download.<pid>` and only renamed into place after
the checksum verifies, so the library path never holds an unverified file. A
soft failure means no native library was installed — `require`/`import` of the
package will fail until one is built or fetched.

## Quick Start

```typescript
import { Container } from 'dependency-injector';

// Define your service interfaces
interface Config {
  debug: boolean;
  port: number;
}

interface Database {
  host: string;
  port: number;
}

// Create container
const container = new Container();

// Register services (automatically serialized as JSON)
container.register<Config>('Config', { debug: true, port: 8080 });
container.register<Database>('Database', { host: 'localhost', port: 5432 });

// Resolve services (automatically deserialized)
const config = container.resolve<Config>('Config');
console.log(config.port); // 8080

// Check existence
console.log(container.contains('Config')); // true

// Don't forget to free when done
container.free();
```

## Scoped Containers

Create child scopes for request-level isolation:

```typescript
const root = new Container();
root.register('Config', { env: 'production' });

// Create request scope
const requestScope = root.scope();
requestScope.register('RequestId', { id: 'req-123' });

// Child can access parent services
requestScope.resolve('Config'); // Works!

// Parent cannot access child services
root.contains('RequestId'); // false

// Clean up
requestScope.free();
root.free();
```

## Removal and Locking

Services can be removed individually or all at once, and a container can be
sealed once wiring is complete:

```typescript
const container = new Container();
container.register('Config', { env: 'production' });
container.register('Cache', { ttl: 60 });

// Remove one service - false means "there was nothing to remove"
container.remove('Cache');   // true
container.remove('Cache');   // false

// Seal the container: no further registrations
container.lock();
container.isLocked();        // true

try {
  container.register('Late', {});
} catch (error) {
  if (error instanceof DIError) {
    console.log(error.code === ErrorCode.Locked); // true
  }
}

// Locking blocks registration only
container.resolve('Config'); // still works
container.remove('Config');  // still works
container.clear();           // still works

// Child scopes start unlocked, even from a locked parent
const child = container.scope();
child.isLocked();            // false
child.register('Request', { id: 'req-1' }); // allowed

child.free();
container.free();
```

## API Reference

### `Container`

#### `new Container()`
Create a new dependency injection container.

#### `container.register<T>(typeName: string, value: T): void`
Register a singleton service. The value is JSON-serialized.

#### `container.resolve<T>(typeName: string): T`
Resolve a service. The value is JSON-deserialized.

#### `container.contains(typeName: string): boolean`
Check if a service is registered. Returns `true`/`false`; a native failure
throws rather than reporting `false` (see
[Error signalling](#error-signalling)).

#### `container.remove(typeName: string): boolean`
Remove a registered service. Returns `true` if it was removed, `false` if no
service with that name was registered. Any other native failure throws.
Permitted on a locked container.

#### `container.clear(): void`
Remove all registered services from this container. Permitted on a locked
container. Child scopes already created keep their own snapshot and are
unaffected.

#### `container.lock(): void`
Lock the container so no further services can be registered. Locking blocks
**registration only** — `remove()`, `clear()` and `resolve()` all keep working.
There is no unlock, and child scopes created with `scope()` start unlocked.
After locking, `register()` throws a `DIError` with `code === ErrorCode.Locked`.

#### `container.isLocked(): boolean`
Check whether the container is locked. A native failure throws rather than
reporting `false` (see [Error signalling](#error-signalling)).

#### `container.scope(): Container`
Create a child scope that inherits parent services. Inheritance is a snapshot
taken at creation time, and the child starts unlocked regardless of the
parent's lock state.

#### `container.serviceCount: number`
Get the number of registered services.

#### `container.free(): void`
Free the container and release native resources.

#### `Container.version(): string`
Get the library version.

### `DIError`

Error class thrown by the container.

```typescript
import { DIError, ErrorCode } from 'dependency-injector';

try {
  container.resolve('NonExistent');
} catch (error) {
  if (error instanceof DIError) {
    console.log(error.code); // ErrorCode.NotFound
    console.log(error.message); // "Service not found: ..."
  }
}
```

### `ErrorCode`

```typescript
enum ErrorCode {
  Ok = 0,
  NotFound = 1,
  InvalidArgument = 2,
  AlreadyRegistered = 3,
  InternalError = 4,
  SerializationError = 5,
  Locked = 6, // Container is locked - registration is not allowed
}
```

An error code the native library returns but this build does not know about
(for example a newer library adding a code) produces a `DIError` carrying that
raw code and the message `Unknown error code: <n>`.

### Error signalling

The native ABI reports the boolean-ish predicates `di_contains` and
`di_is_locked` as an `int32_t`: `1` for true, `0` for false, and **`-1` for
"an error occurred"** — an internal error or an invalid argument, with the
detail available from `di_error_message()`.

`-1` is *not* false. Collapsing it would report an internal failure as a
confident "the service is not registered" / "the container is not locked", so
this binding throws a `DIError` (`ErrorCode.InternalError`, carrying the native
message) instead:

```typescript
try {
  if (container.contains('Config')) {
    // ...
  }
} catch (error) {
  if (error instanceof DIError) {
    // A genuine failure, distinct from "not registered".
    console.error(error.message);
  }
}
```

`remove()` is the one place where a non-`Ok` code is folded into a boolean, and
only for `NotFound`: "there was nothing to remove" is an outcome, not a
failure. Every other code throws.

## Development

### Setup

```bash
# Install dependencies (pnpm required)
cd ffi/nodejs
pnpm install
```

### Build

```bash
# Full build (SWC + TypeScript declarations)
pnpm build

# SWC only (fast JS compilation)
pnpm build:swc

# TypeScript declarations only
pnpm build:types

# Type checking without emit
pnpm typecheck
```

### Running Tests

```bash
# Build the native library first
cd /path/to/dependency-injector
cargo rustc --release --features ffi --crate-type cdylib

# Run tests
cd ffi/nodejs
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
pnpm test
```

### Running the Example

```bash
cd ffi/nodejs
pnpm install
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
pnpm example
```

## Type Safety

The library uses TypeScript generics for type-safe resolution:

```typescript
interface User {
  id: number;
  name: string;
}

container.register<User>('User', { id: 1, name: 'Alice' });

// TypeScript knows this is a User
const user = container.resolve<User>('User');
console.log(user.name); // TypeScript autocomplete works!
```

## Memory Management

**Important**: Always call `free()` on containers when you're done:

```typescript
const container = new Container();
try {
  // Use container...
} finally {
  container.free();
}
```

For scoped containers:

```typescript
const root = new Container();
const scope = root.scope();

// Free in reverse order (children before parents)
scope.free();
root.free();
```

## How It Works

This package uses [koffi](https://koffi.dev/) for FFI bindings, which:
- Requires no native compilation (unlike `ffi-napi`)
- Works out of the box on Windows, macOS, and Linux
- Supports all modern Node.js versions (18+)

Services are serialized as JSON, which means:
- Plain objects, arrays, strings, numbers, and booleans work perfectly
- Class instances and functions cannot be serialized
- Complex nested structures are fully supported

## Performance

The native Rust library achieves ~9ns singleton resolution. The FFI overhead adds:
- ~50-100ns for JSON serialization
- ~10-20ns for FFI call overhead

For most applications, this is negligible. If you need maximum performance, consider using the Rust library directly.

## Limitations

- Services are JSON-serialized, so functions and class instances won't work
- The native library must be built and accessible via LD_LIBRARY_PATH
- Binary data should be base64-encoded in JSON

## License

MIT OR Apache-2.0

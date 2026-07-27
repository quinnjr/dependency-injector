# dependency-injector C# Bindings

C# bindings for the high-performance Rust dependency injection container.

## Features

- 🚀 **High Performance** - Native Rust implementation with ~10ns resolution
- 📦 **Type-Safe** - Full generic support with compile-time type checking
- 🔄 **Scoped Containers** - Hierarchical scopes for request-level isolation
- 🧵 **Thread-Safe** - Safe to use from multiple threads
- 🔌 **P/Invoke** - Direct native bindings via P/Invoke
- 📥 **Pre-built Natives** - Native libraries bundled for all major platforms

## Installation

```bash
dotnet add package PegasusHeavy.DependencyInjector
```

The NuGet package includes pre-built native libraries for:

| Platform | Architecture |
|----------|--------------|
| Linux | x64, arm64 |
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Windows | x64 |

### Manual Build (Optional)

If you want to build the native library from source:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/pegasusheavy/dependency-injector
cd dependency-injector
cargo rustc --release --features ffi --crate-type cdylib

# Point to your build
export DI_LIBRARY_PATH=$(pwd)/target/release/libdependency_injector.so
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `DI_LIBRARY_PATH` | Custom path to native library |

### Debugging Library Loading

You can check which library was loaded:

```csharp
Console.WriteLine($"Library path: {Container.LibraryPath}");
Console.WriteLine($"Library version: {Container.Version}");
```

## Quick Start

```csharp
using DependencyInjector;

// Define your service types
record Config(bool Debug, int Port, string Environment);
record User(int Id, string Name, string Email);

// Create a container
using var container = new Container();

// Register services
container.Register("Config", new Config(true, 8080, "development"));
container.Register(new User(1, "Alice", "alice@example.com"));

// Resolve services
var config = container.Resolve<Config>("Config");
Console.WriteLine($"Port: {config.Port}"); // Port: 8080

var user = container.Resolve<User>(); // Uses type name as key
Console.WriteLine($"User: {user.Name}"); // User: Alice

// Optional resolution (returns null if not found)
var missing = container.TryResolve<Config>("NonExistent");
// missing is null

// Check existence
Console.WriteLine(container.Contains("Config")); // true
```

## Scoped Containers

Create child scopes for request-level isolation:

```csharp
using var root = new Container();
root.Register("Config", new Config(true, 8080, "production"));

// Create request scope
using var request = root.Scope();
request.Register("RequestId", new RequestContext("req-123"));

// Child can access parent services
var config = request.Resolve<Config>("Config"); // Works!

// Parent cannot access child services
root.Contains("RequestId"); // false

// Clean up happens automatically with `using`
```

## API Reference

### Container

```csharp
// Create container
using var container = new Container();

// Register with explicit type name
container.Register<T>("TypeName", instance);

// Register using type's full name
container.Register<T>(instance);

// Resolve (throws DIException if not found)
T service = container.Resolve<T>("TypeName");
T service = container.Resolve<T>();

// Try resolve (returns null if not found)
T? service = container.TryResolve<T>("TypeName");
T? service = container.TryResolve<T>();

// Check existence (throws DIException if the native call reports an error)
bool exists = container.Contains("TypeName");
bool exists = container.Contains<T>();

// Remove a service (true if removed, false if it was not registered)
bool removed = container.Remove("TypeName");
bool removed = container.Remove<T>();

// Remove every service
container.Clear();

// Lock against further registration (no unlock)
container.Lock();

// Query the lock state (throws DIException if the native call reports an error)
bool locked = container.IsLocked;

// Get service count
long count = container.ServiceCount;

// Create child scope
using var scope = container.Scope();

// Get library version
string version = Container.Version;

// Dispose (called automatically with `using`)
container.Dispose();
```

### DIException

```csharp
using DependencyInjector;
using DependencyInjector.Native;

try
{
    container.Resolve<Config>("NonExistent");
}
catch (DIException ex)
{
    Console.WriteLine(ex.ErrorCode); // DiErrorCode.NotFound
    Console.WriteLine(ex.Message);   // "Service 'NonExistent' not found"
}
```

### Error Codes

| Code | Enum Value | Description |
|------|------------|-------------|
| 0 | `DiErrorCode.Ok` | Success |
| 1 | `DiErrorCode.NotFound` | Service not found |
| 2 | `DiErrorCode.InvalidArgument` | Invalid argument |
| 3 | `DiErrorCode.AlreadyRegistered` | Service already exists |
| 4 | `DiErrorCode.InternalError` | Internal error |
| 5 | `DiErrorCode.SerializationError` | JSON serialization failed |
| 6 | `DiErrorCode.Locked` | Container is locked - registration is not allowed |

The enum is append-only: existing values never get renumbered. A code emitted
by a newer native library than this binding knows about is not a cast or switch
failure - it surfaces as a `DIException` whose `ErrorCode` carries the raw
numeric value and whose message reads `Unknown error: <n>`.

## Removal and Locking

```csharp
using var container = new Container();
container.Register("Config", new Config(true, 8080, "production"));

Console.WriteLine(container.IsLocked); // false
container.Lock();
Console.WriteLine(container.IsLocked); // true

// Locking blocks registration only...
try
{
    container.Register("Late", new Config(false, 9090, "development"));
}
catch (DIException ex)
{
    Console.WriteLine(ex.ErrorCode); // DiErrorCode.Locked
}

// ...removal and clearing stay permitted on a locked container.
container.Remove("Config"); // true
container.Clear();
```

There is no unlock. Child containers created with `Scope()` start unlocked
regardless of the parent's lock state.

## Error Sentinels: `-1` Means Error, Not `false`

The native `di_contains` and `di_is_locked` entry points are tri-state: they
return `1` (yes), `0` (no), or `-1` (internal error, invalid argument, or a
caught panic). Per the C header, callers must **not** collapse `-1` into
`false` - doing so reports a genuine failure as "not registered" / "not
locked".

This binding surfaces the negative case explicitly:

```csharp
try
{
    if (container.Contains("Config")) { /* ... */ }
}
catch (DIException ex)
{
    // ex.ErrorCode == DiErrorCode.InternalError
    // ex.Message   == the native last-error message
}
```

`Contains(string)`, `Contains<T>()`, and `IsLocked` therefore each have three
outcomes - `true`, `false`, or a thrown `DIException` populated from
`di_error_message()`.

## Project Structure

```
ffi/csharp/
├── DependencyInjector.sln           # Solution file
├── DependencyInjector/              # Main library
│   ├── Container.cs                 # High-level container API
│   ├── NativeBindings.cs            # P/Invoke declarations
│   └── DependencyInjector.csproj
├── DependencyInjector.Tests/        # Unit tests
│   ├── ContainerTests.cs
│   └── DependencyInjector.Tests.csproj
├── Example/                         # Example application
│   ├── Program.cs
│   └── Example.csproj
└── README.md
```

## Building and Testing

```bash
cd ffi/csharp

# Build all projects
dotnet build

# Run tests
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
dotnet test

# Run example
dotnet run --project Example
```

## Performance

The native Rust library achieves ~10ns singleton resolution. The P/Invoke overhead adds:
- ~50-100ns for P/Invoke call
- ~1-5µs for JSON serialization (System.Text.Json)

### Benchmark Comparison

| Library | Singleton Resolution | Mixed Workload (100 ops) |
|---------|---------------------|--------------------------|
| **Rust dependency-injector** | **17-32 ns** | **2.2 µs** |
| C# Dictionary | 142 ns | 30 µs |
| Microsoft.Extensions.DI | 208 ns | 31 µs |
| C# Manual DI | 393 ns | 3.4 µs |

*Note: C# benchmarks include JIT compilation overhead. Production performance may vary.*

## Limitations

- Services are JSON-serialized, so:
  - Functions and delegates won't work
  - Circular references may cause issues
  - Non-serializable types need custom handling
- The native library must be accessible via LD_LIBRARY_PATH or in the executable directory
- Requires .NET 8.0 or later (for modern P/Invoke features)

## Memory Management

The container implements `IDisposable` and uses a finalizer for safety. Always use `using` statements:

```csharp
using var container = new Container();
// Use container...
// Automatically disposed at end of scope
```

For scoped containers, the `using` statement handles cleanup in the correct order:

```csharp
using var root = new Container();
using var scope = root.Scope();
// Use containers...
// scope is disposed first, then root
```

## License

MIT OR Apache-2.0

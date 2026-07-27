# FFI Bindings for dependency-injector

This directory contains FFI (Foreign Function Interface) bindings that allow the Rust dependency injection container to be used from other languages.

## Supported Languages

- **C/C++** - via `dependency_injector.h`
- **C#** - via `csharp/` package (P/Invoke, NuGet: `PegasusHeavy.DependencyInjector`)
- **Go** - via `go/di` package
- **Node.js/TypeScript** - via `nodejs/` package
- **Python** - via `python/` package

## Building the Shared Library

```bash
# From the project root - build as cdylib for FFI
cargo rustc --release --features ffi --crate-type cdylib

# The shared library will be at:
# Linux:   target/release/libdependency_injector.so
# macOS:   target/release/libdependency_injector.dylib
# Windows: target/release/dependency_injector.dll
```

> **Note**: The `--crate-type cdylib` flag is required to build the shared library.
> Without it, only the Rust static library (`.rlib`) is built.

## C/C++ Usage

Include the header file and link against the shared library:

```c
#include "dependency_injector.h"

int main() {
    DiContainer* container = di_container_new();

    // Register a service
    const char* json = "{\"name\": \"MyService\", \"port\": 8080}";
    di_register_singleton_json(container, "Config", json);

    // Resolve the service
    DiResult result = di_resolve(container, "Config");
    if (result.code == DI_OK) {
        const uint8_t* data = di_service_data(result.service);
        size_t len = di_service_data_len(result.service);
        // Use the data...
        di_service_free(result.service);
    }

    di_container_free(container);
    return 0;
}
```

Compile with:
```bash
gcc -o myapp myapp.c -L/path/to/target/release -ldependency_injector -I/path/to/ffi
```

## C# Usage

### Setup

1. Install the NuGet package (bundles pre-built native libraries):
   ```bash
   dotnet add package PegasusHeavy.DependencyInjector
   ```

   Or build the native library from source and point to it:
   ```bash
   cargo rustc --release --features ffi --crate-type cdylib
   export DI_LIBRARY_PATH=$(pwd)/target/release/libdependency_injector.so
   ```

### Example

```csharp
using DependencyInjector;

record Config(bool Debug, int Port);

// Create container (IDisposable - freed at end of scope)
using var container = new Container();

// Register services (JSON serialization)
container.Register("Config", new Config(Debug: true, Port: 8080));

// Resolve services
var config = container.Resolve<Config>("Config");
Console.WriteLine(config.Port); // 8080

// Optional resolution and lookups
var missing = container.TryResolve<Config>("NonExistent"); // null
Console.WriteLine(container.Contains("Config")); // True

// Scoped containers
using var requestScope = container.Scope();
requestScope.Register("RequestId", new { Id = "req-123" });
```

### Running the Example

```bash
cd ffi/csharp
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
dotnet run --project Example
```

## Go Usage

### Setup

1. Build the Rust shared library:
   ```bash
   cargo rustc --release --features ffi --crate-type cdylib
   ```

2. Set the library path:
   ```bash
   # Linux
   export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH

   # macOS
   export DYLD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$DYLD_LIBRARY_PATH
   ```

3. Import the package:
   ```go
   import "github.com/pegasusheavy/dependency-injector/ffi/go/di"
   ```

### Example

```go
package main

import (
    "fmt"
    "log"

    "github.com/pegasusheavy/dependency-injector/ffi/go/di"
)

type Config struct {
    Debug bool   `json:"debug"`
    Port  int    `json:"port"`
}

func main() {
    // Create container
    container := di.NewContainer()
    defer container.Free()

    // Register a struct
    err := container.RegisterValue("Config", Config{Debug: true, Port: 8080})
    if err != nil {
        log.Fatal(err)
    }

    // Resolve it back
    var config Config
    err = container.ResolveJSON("Config", &config)
    if err != nil {
        log.Fatal(err)
    }

    fmt.Printf("Config: debug=%v, port=%d\n", config.Debug, config.Port)

    // Create a scoped container
    requestScope, _ := container.Scope()
    defer requestScope.Free()

    // Scope inherits parent services (Contains returns (bool, error);
    // a negative native sentinel is surfaced as an error, never as false)
    hasConfig, _ := requestScope.Contains("Config")
    fmt.Printf("Has Config: %v\n", hasConfig)
}
```

### Running Tests

```bash
cd ffi/go/di
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
go test -v
```

### Running the Example

```bash
cd ffi/go/example
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
go run main.go
```

## Node.js/TypeScript Usage

### Setup

1. Build the Rust shared library:
   ```bash
   cargo rustc --release --features ffi --crate-type cdylib
   ```

2. Set the library path:
   ```bash
   export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
   ```

3. Install the package:
   ```bash
   cd ffi/nodejs
   pnpm install
   ```

### Example

```typescript
import { Container } from '@pegasusheavy/dependency-injector';

interface Config {
  debug: boolean;
  port: number;
}

// Create container
const container = new Container();

// Register services (JSON serialization)
container.register<Config>('Config', { debug: true, port: 8080 });

// Resolve services
const config = container.resolve<Config>('Config');
console.log(config.port); // 8080

// Scoped containers
const requestScope = container.scope();
requestScope.register('RequestId', { id: 'req-123' });

// Clean up
requestScope.free();
container.free();
```

### Running the Example

```bash
cd ffi/nodejs
pnpm run example
```

---

## Python Usage

### Setup

1. Build the Rust shared library:
   ```bash
   cargo rustc --release --features ffi --crate-type cdylib
   ```

2. Set the library path:
   ```bash
   export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
   ```

3. Install the package:
   ```bash
   cd ffi/python
   pip install -e .
   ```

### Example

```python
from dependency_injector import Container

# Create container
container = Container()

# Register services (JSON serialization)
container.register("Config", {"debug": True, "port": 8080})

# Resolve services
config = container.resolve("Config")
print(config["port"])  # 8080

# Scoped containers
with container.scope() as request:
    request.register("RequestId", {"id": "req-123"})
    ctx = request.resolve("RequestId")

# Clean up
container.free()
```

### Running the Example

```bash
cd ffi/python
python examples/basic.py
```

---

## API Reference

### Container Functions

| Function | Description |
|----------|-------------|
| `di_container_new()` | Create a new container |
| `di_container_free(container)` | Free a container |
| `di_container_scope(container)` | Create a child scope |

### Registration Functions

| Function | Description |
|----------|-------------|
| `di_register_singleton(container, type_name, data, len)` | Register with raw bytes |
| `di_register_singleton_json(container, type_name, json)` | Register with JSON string |

### Resolution Functions

| Function | Description |
|----------|-------------|
| `di_resolve(container, type_name)` | Resolve a service |
| `di_contains(container, type_name)` | Check if registered |
| `di_service_count(container)` | Get number of services |

### Service Data Access

| Function | Description |
|----------|-------------|
| `di_service_data(service)` | Get data pointer |
| `di_service_data_len(service)` | Get data length |
| `di_service_type_name(service)` | Get type name |
| `di_service_free(service)` | Free service handle |

### Error Handling

| Function | Description |
|----------|-------------|
| `di_error_message()` | Get last error message |
| `di_error_clear()` | Clear last error |
| `di_string_free(s)` | Free string from library |

### Utility

| Function | Description |
|----------|-------------|
| `di_version()` | Get library version |

## Memory Management

- **Containers**: Created with `di_container_new()`, must be freed with `di_container_free()`
- **Services**: Returned by `di_resolve()`, must be freed with `di_service_free()`
- **Strings**: From `di_error_message()` or `di_service_type_name()`, must be freed with `di_string_free()`
- **Version string**: From `di_version()` is static, do NOT free

## Thread Safety

The container is fully thread-safe. All functions can be called from multiple threads concurrently.

## Error Codes

| Code | Name | Description |
|------|------|-------------|
| 0 | `DI_OK` | Success |
| 1 | `DI_NOT_FOUND` | Service not found |
| 2 | `DI_INVALID_ARGUMENT` | Invalid argument |
| 3 | `DI_ALREADY_REGISTERED` | Service already exists |
| 4 | `DI_INTERNAL_ERROR` | Internal error |
| 5 | `DI_SERIALIZATION_ERROR` | Serialization failed |


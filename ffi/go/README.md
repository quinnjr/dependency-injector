# dependency-injector Go Bindings

Go bindings for the high-performance Rust dependency injection container.

## Features

- 🚀 **High Performance** - Native Rust implementation with ~10ns resolution
- 🔄 **Scoped Containers** - Hierarchical scopes for request-level isolation
- 📝 **JSON Support** - Easy serialization with Go structs
- 🧵 **Thread-Safe** - Safe to use from multiple goroutines
- 🔌 **cgo-Based** - Direct native bindings via cgo

## Prerequisites

1. **Go 1.21+** with cgo enabled

2. Build the Rust library:

```bash
cd /path/to/dependency-injector
cargo rustc --release --features ffi --crate-type cdylib
```

3. Set the library path:

```bash
# Linux
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH

# macOS
export DYLD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$DYLD_LIBRARY_PATH

# Windows (add to PATH)
set PATH=%PATH%;C:\path\to\dependency-injector\target\release
```

## Installation

```bash
go get github.com/quinnjr/dependency-injector/ffi/go/di
```

## Quick Start

```go
package main

import (
    "fmt"
    "log"

    "github.com/quinnjr/dependency-injector/ffi/go/di"
)

type Config struct {
    Debug bool   `json:"debug"`
    Port  int    `json:"port"`
    Host  string `json:"host"`
}

func main() {
    // Create container
    container := di.NewContainer()
    defer container.Free()

    // Register services using Go structs
    config := Config{Debug: true, Port: 8080, Host: "localhost"}
    if err := container.RegisterValue("Config", config); err != nil {
        log.Fatal(err)
    }

    // Resolve services
    var resolved Config
    if err := container.ResolveJSON("Config", &resolved); err != nil {
        log.Fatal(err)
    }

    fmt.Printf("Config: %+v\n", resolved)
    // Config: {Debug:true Port:8080 Host:localhost}
}
```

## Scoped Containers

Create child scopes for request-level isolation:

```go
// Root container with app-wide services
root := di.NewContainer()
defer root.Free()

root.RegisterValue("Config", Config{Env: "production"})

// Create request scope
request, _ := root.Scope()
defer request.Free()

request.RegisterValue("RequestID", "req-123")

// Child can access parent services
var config Config
request.ResolveJSON("Config", &config) // Works!

// Parent cannot access child services
found, err := root.Contains("RequestID") // false, nil
```

## Locking

Lock a container once startup registration is finished. Locking blocks
registration only — removal and clearing stay permitted, and child scopes
created with `Scope()` start unlocked.

```go
container := di.NewContainer()
defer container.Free()

container.RegisterValue("Config", config)
container.Lock()

// Registration is refused from here on
err := container.RegisterValue("Late", late)
if errors.Is(err, di.ErrLocked) {
    // container is locked
}

// Removal and clearing still work
removed, _ := container.Remove("Config") // true, nil
container.Clear()

locked, _ := container.IsLocked() // true, nil (there is no unlock)
```

## API Reference

### Container

```go
// Create a new container
container := di.NewContainer()

// Register raw bytes
container.Register("Key", []byte("data"))

// Register JSON string
container.RegisterJSON("Key", `{"field": "value"}`)

// Register Go value (auto-serialized to JSON)
container.RegisterValue("Key", myStruct)

// Resolve to raw bytes
data, err := container.Resolve("Key")

// Resolve into Go struct
var target MyStruct
err := container.ResolveInto("Key", &target)
// or
err := container.ResolveJSON("Key", &target)

// Try resolve (returns nil if not found, no error)
data := container.TryResolve("Key")

// Check existence. The native di_contains returns -1 on an internal error or
// invalid argument; that case is reported as a non-nil error instead of being
// collapsed into false, so always check err before trusting exists.
exists, err := container.Contains("Key")

// Remove a single service. removed is false (with a nil error) when no
// service was registered under that name.
removed, err := container.Remove("Key")

// Remove every registered service
err := container.Clear()

// Prevent further registrations. Removal and clearing remain permitted and
// there is no unlock.
err := container.Lock()

// Check the lock state. As with Contains, the -1 error case is surfaced as a
// non-nil error rather than collapsed into false.
locked, err := container.IsLocked()

// Get service count
count := container.ServiceCount()

// Create child scope (always starts unlocked)
child, err := container.Scope()

// Free resources (also called by finalizer)
container.Free()

// Get library version
version := di.Version()
```

### Error Handling

```go
import "errors"

data, err := container.Resolve("NonExistent")
if err != nil {
    var diErr *di.DIError
    if errors.As(err, &diErr) {
        switch diErr.Code {
        case di.NotFound:
            // Handle not found
        case di.AlreadyRegistered:
            // Handle duplicate
        case di.Locked:
            // Handle registration on a locked container
        }
    }
}

// Or use sentinel errors
if errors.Is(err, di.ErrNotFound) {
    // Handle not found
}
if errors.Is(err, di.ErrLocked) {
    // Registration was refused because the container is locked
}
```

### Error Codes

| Code | Constant | Description |
|------|----------|-------------|
| 0 | `OK` | Success |
| 1 | `NotFound` | Service not found |
| 2 | `InvalidArgument` | Invalid argument |
| 3 | `AlreadyRegistered` | Service already exists |
| 4 | `InternalError` | Internal error |
| 5 | `SerializationError` | JSON serialization failed |
| 6 | `Locked` | Container is locked - registration is not allowed |

Unrecognized codes from a newer native library are reported as
`unknown error code: N` rather than being mapped onto a known code.

## Running Tests

```bash
cd ffi/go/di
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
go test -v
```

## Running Benchmarks

```bash
cd ffi/go/di
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
go test -bench=. -benchmem
```

## Running the Example

```bash
cd ffi/go/example
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
go run main.go
```

## Memory Management

The container uses Go's finalizer to automatically free resources, but it's recommended to call `Free()` explicitly:

```go
container := di.NewContainer()
defer container.Free() // Always defer Free()

// Use container...
```

For scoped containers, free children before parents:

```go
root := di.NewContainer()
defer root.Free()

child, _ := root.Scope()
defer child.Free()

// Use containers...
// child.Free() is called first due to defer order
```

## Performance

The native Rust library achieves ~10ns singleton resolution. The cgo overhead adds:
- ~50-100ns for cgo call
- ~1-5µs for JSON marshaling (when using `ResolveJSON`)

For most applications, this is negligible compared to actual I/O operations.

## Limitations

- Requires cgo (not compatible with `CGO_ENABLED=0`)
- Services are JSON-serialized, so functions and channels won't work
- The native library must be accessible via LD_LIBRARY_PATH

## License

MIT OR Apache-2.0




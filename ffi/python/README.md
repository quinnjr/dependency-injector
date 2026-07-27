# dependency-injector-rust

Python bindings for the high-performance Rust dependency injection container.

## Features

- 🚀 **High Performance** - Native Rust implementation with ~10ns resolution
- 🐍 **Pythonic API** - Clean, idiomatic Python interface
- 🔄 **Scoped Containers** - Hierarchical scopes for request-level isolation
- 📝 **Type Hints** - Full type annotation support with PEP 561
- 🔌 **Zero Dependencies** - Uses only Python's built-in `ctypes`
- 📥 **Pre-built Wheels** - Native libraries bundled for all major platforms

## Installation

```bash
pip install dependency-injector-rust
```

Pre-built wheels are available for:

| Platform | Architecture |
|----------|--------------|
| Linux | x86_64, aarch64 |
| macOS | x86_64 (Intel), arm64 (Apple Silicon) |
| Windows | x86_64 |

### Manual Build (Optional)

If pre-built wheels aren't available for your platform, or you want to build from source:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/pegasusheavy/dependency-injector
cd dependency-injector
cargo rustc --release --features ffi --crate-type cdylib

# Install Python package
pip install ./ffi/python

# Or point to your build
export DI_LIBRARY_PATH=$(pwd)/target/release/libdependency_injector.so
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `DI_LIBRARY_PATH` | Custom path to native library |
| `DI_SKIP_DOWNLOAD` | Skip automatic download (for offline/CI) |
| `DI_GITHUB_TOKEN` | GitHub token for download rate limiting. Only ever sent to `api.github.com` — it is stripped on any cross-host redirect and never sent to download hosts |
| `DI_REQUIRE_CHECKSUM` | When set (non-empty), a release with no `SHA256SUMS` asset is a hard failure (exit 1) instead of a warning |

### Install Failure Policy

The downloader distinguishes availability problems (soft) from integrity
problems (hard):

| Situation | Behaviour |
|-----------|-----------|
| Release metadata fetch fails, platform asset missing from the release, download fails, or the transfer is truncated | Warn, print build-from-source instructions, **exit 0** |
| Checksum mismatch, `SHA256SUMS` present but with no entry for the asset, or an existing `SHA256SUMS` asset cannot be fetched | **Exit 1**; the downloaded file is deleted |
| Release has no `SHA256SUMS` asset at all (pre-checksum release) | Warn and proceed — unless `DI_REQUIRE_CHECKSUM` is set, then **exit 1** |
| Unsupported platform | **Exit 1** |

Downloads are staged next to the final path and only moved into place after the
checksum verifies, so the library path never holds an unverified or partial
file. A truncated transfer is reported as a network error, never as tampering.

### Download Pre-built Library

If you installed from source distribution (sdist), you can download the native library:

```bash
python -m scripts.download_native
```

**Exit-code contract:** this command exits `0` even when no library was
downloaded (a missing asset or a network failure is non-fatal, so it never
breaks an install). It prints `NO NATIVE LIBRARY WAS INSTALLED` in that case —
do not read a `0` exit as proof the library is present, and do not rely on a
`&& python -c "import dependency_injector"` chain to catch it, since the chain
will simply run and fail on its own. A non-zero exit means an integrity failure
or an unsupported platform. For strict mode, set `DI_REQUIRE_CHECKSUM=1` so an
unverifiable (pre-checksum) release fails hard instead of proceeding.

## Quick Start

```python
from dependency_injector import Container

# Create container
container = Container()

# Register services (automatically JSON-serialized)
container.register("Config", {"debug": True, "port": 8080})
container.register("Database", {"host": "localhost", "port": 5432})

# Resolve services (automatically JSON-deserialized)
config = container.resolve("Config")
print(config["port"])  # 8080

# Check existence
print(container.contains("Config"))  # True

# Don't forget to free!
container.free()
```

## Context Manager Support

```python
from dependency_injector import Container

with Container() as container:
    container.register("Service", {"data": "value"})
    result = container.resolve("Service")
    print(result["data"])  # "value"
# Container is automatically freed
```

## Scoped Containers

Create child scopes for request-level isolation:

```python
from dependency_injector import Container

root = Container()
root.register("Config", {"env": "production"})

# Create request scope
request = root.scope()
request.register("RequestId", {"id": "req-123"})

# Child can access parent services
config = request.resolve("Config")  # Works!

# Parent cannot access child services
root.contains("RequestId")  # False

# Clean up (children before parents)
request.free()
root.free()
```

## Optional Resolution

Use `try_resolve` to get `None` instead of raising an exception for missing services:

```python
from dependency_injector import Container

container = Container()
container.register("Config", {"debug": True})

# Returns the value if found
config = container.try_resolve("Config")  # {"debug": True}

# Returns None if not found (no exception)
missing = container.try_resolve("NonExistent")  # None

container.free()
```

## Type Hints with TypedDict

```python
from typing import TypedDict
from dependency_injector import Container

class Config(TypedDict):
    debug: bool
    port: int

container = Container()
container.register("Config", Config(debug=True, port=8080))

config: Config = container.resolve("Config")
print(config["port"])  # IDE knows this is an int!

container.free()
```

## API Reference

### `Container`

```python
from dependency_injector import Container

container = Container()

# Register a service (JSON-serializable value)
container.register("Key", {"value": 1})

# Resolve a service (raises DIError if not found)
data = container.resolve("Key")  # {"value": 1}

# Try to resolve (returns None if not found)
data = container.try_resolve("Key")  # {"value": 1} or None

# Check if a service exists
container.contains("Key")  # True

# Get service count
container.service_count  # 1

# Create a child scope
child = container.scope()

# Get library version
Container.version()  # "0.2.2"

# Free resources
container.free()
```

### Error Handling

```python
from dependency_injector import Container, DIError, ErrorCode

container = Container()

try:
    container.resolve("NonExistent")
except DIError as e:
    print(e.code)     # ErrorCode.NOT_FOUND
    print(e.message)  # "Service 'NonExistent' not found"

container.free()
```

### Error Codes

| Code | Name | Description |
|------|------|-------------|
| 0 | `OK` | Success |
| 1 | `NOT_FOUND` | Service not found |
| 2 | `INVALID_ARGUMENT` | Invalid argument |
| 3 | `ALREADY_REGISTERED` | Service already exists |
| 4 | `INTERNAL_ERROR` | Internal error |
| 5 | `SERIALIZATION_ERROR` | JSON serialization failed |

## Running Tests

```bash
cd ffi/python
pip install -e ".[dev]"
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
pytest tests/ -v
```

## Running the Example

```bash
cd ffi/python
export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
python examples/basic.py
```

## How It Works

This library uses Python's built-in `ctypes` module to call the Rust FFI functions directly. Services are serialized as JSON, which means:

- Plain objects (dicts), lists, strings, numbers, and booleans work perfectly
- Class instances and functions cannot be serialized
- Complex nested structures are fully supported
- Use `dataclasses.asdict()` to serialize dataclasses

## Performance

The native Rust library achieves ~10ns singleton resolution. The FFI overhead adds:

- ~1-5µs for JSON serialization (Python's `json` module)
- Minimal overhead for the ctypes FFI call

For most applications, this is negligible compared to actual I/O operations.

## Limitations

- Services are JSON-serialized, so functions and class instances won't work
- The native library must be accessible via LD_LIBRARY_PATH
- Binary data should be base64-encoded in JSON

## License

MIT OR Apache-2.0

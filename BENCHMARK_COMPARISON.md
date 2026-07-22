# Dependency Injector: Cross-Language Benchmark Comparison

> **⚠️ Mixed-workload figures pending re-measurement (2026-07-21):** the
> comparison benchmarks' "5% scope creation" branches previously under-worked
> for samber/do, inversify, and the Python libraries (no real scope was
> created). The benchmark code has been fixed; the mixed-workload numbers
> below for those libraries predate the fix and will change when re-run.

Comprehensive benchmarks comparing Rust `dependency-injector` against popular Go, Node.js, Python, and C# DI libraries.

**Test Environment:**
- CPU: Intel Core i9-13900K (32 threads)
- OS: Linux (WSL2)
- Rust: 1.85 (release mode)
- Go: 1.24

---

## Go DI Libraries Compared

| Library | Version | Type | Description |
|---------|---------|------|-------------|
| **sync.Map** | stdlib | Runtime | Go's concurrent-safe map |
| **map+RWMutex** | stdlib | Runtime | Traditional mutex-protected map |
| **goioc/di** | 1.7.1 | Runtime | IoC container |
| **samber/do** | 2.0.0 | Runtime | Generic DI with Go 1.18+ generics |
| **uber-go/dig** | 1.19.0 | Runtime | Uber's reflection-based DI |

---

## Benchmark Results

### 1. Singleton Resolution (Single Service Lookup)

The most common DI operation - resolving a pre-registered singleton.

| Library | Language | Time | Allocations | vs Fastest |
|---------|----------|------|-------------|------------|
| **Go manual** | Go | 0.5 ns | 0 | 1.0x |
| **Go sync.Map** | Go | 15-29 ns | 0 | 30-58x |
| **Go map+RWMutex** | Go | 25-28 ns | 0 | 50-56x |
| **Go goioc/di** | Go | 109-171 ns | 0 | 218-342x |
| **Go samber/do** | Go | 767-844 ns | 6 | 1534-1688x |
| **Go uber/dig** | Go | 4,214-6,409 ns | 25 | 8428-12818x |
| | | | | |
| **Rust manual** | Rust | ~1 ns | 0 | ~2x |
| **Rust dependency-injector** | Rust | 17-32 ns | 0 | 34-64x |
| **Rust HashMap+RwLock** | Rust | 60-73 ns | 0 | 120-146x |
| **Rust DashMap** | Rust | 84-123 ns | 0 | 168-246x |

**Key Insights:**
- Go's `sync.Map` and Rust's `dependency-injector` are competitive (~15-30ns)
- Go's popular DI libraries (samber/do, uber/dig) are significantly slower due to reflection
- Rust's `dependency-injector` with hot cache can achieve ~8-10ns on cache hits

---

### 2. Deep Dependency Chain (Service with Dependencies)

Resolving a service that has multiple levels of dependencies.

| Library | Language | Time | Allocations |
|---------|----------|------|-------------|
| **Go manual** | Go | 0.15-0.18 ns | 0 |
| **Go sync.Map** | Go | 11-14 ns | 0 |
| **Go map+RWMutex** | Go | 16-18 ns | 0 |
| **Go samber/do** | Go | 276-498 ns | 6 |
| **Go uber/dig** | Go | 1,144-1,315 ns | 25 |
| | | | |
| **Rust dependency-injector** | Rust | 16-17 ns | 0 |
| **Rust shaku** | Rust | 16-17 ns | 0 |
| **Rust HashMap+RwLock** | Rust | 45-50 ns | 0 |

**Key Insights:**
- Both Go `sync.Map` and Rust `dependency-injector` achieve ~11-17ns for cached lookups
- Go's reflection-based libraries have significant overhead for dependency resolution
- Pre-cached singletons make dependency depth irrelevant for performance

---

### 3. Container Creation

Creating a new DI container instance.

| Library | Language | Time | Allocations |
|---------|----------|------|-------------|
| **Go sync.Map** | Go | 0.3-0.5 ns | 0 |
| **Go map+RWMutex** | Go | 6.5-10 ns | 0 |
| **Go manual** | Go | 0.9-1.0 ns | 0 |
| **Go samber/do** | Go | 27-228 µs | 30 |
| **Go uber/dig** | Go | 63-179 µs | 51 |
| | | | |
| **Rust HashMap+RwLock** | Rust | 10 ns | 0 |
| **Rust shaku** | Rust | 179-188 ns | 0 |
| **Rust dependency-injector** | Rust | 434-740 ns | 0 |
| **Rust DashMap** | Rust | 1.6-1.8 µs | 0 |

**Key Insights:**
- Go's stdlib containers are extremely fast to create
- Rust's `dependency-injector` has moderate setup cost due to DashMap shards
- Container creation is typically a one-time startup cost

---

### 4. Concurrent Access (Parallel Reads)

Performance under concurrent read load (32 goroutines/threads).

| Library | Language | Time/op |
|---------|----------|---------|
| **Go sync.Map** | Go | 0.9-1.3 ns |
| **Go map+RWMutex** | Go | 51-68 ns |
| **Go uber/dig** | Go | 1,299-54,752 ns |
| **Go samber/do** | Go | 1,081-26,952 ns |
| | | |
| **Rust dependency-injector** | Rust | 1.3-3.4 ms (100 ops) |
| **Rust HashMap+RwLock** | Rust | 5.7 ms (100 ops) |

**Key Insights:**
- Go's `sync.Map` excels at concurrent read access (~1ns per operation)
- Rust's DashMap-based implementation scales well but has higher overhead
- Both languages benefit from lock-free data structures

---

### 5. Mixed Workload (100 Operations)

Simulating realistic usage: 80% resolutions, 15% lookups, 5% scope creation.

| Library | Language | Time | Allocations |
|---------|----------|------|-------------|
| **Go map+RWMutex** | Go | 7-133 µs | 20 |
| **Go sync.Map** | Go | 9-31 µs | 25 |
| **Go samber/do** | Go | 125-1,399 µs | 570 |
| | | | |
| **Rust dependency-injector** | Rust | 2.2 µs | 0 |
| **Rust DashMap basic** | Rust | 5.9-6.0 µs | 0 |
| **Rust shaku** | Rust | 2.5-15 µs | 0 |

**Key Insights:**
- **Rust `dependency-injector` wins with consistent 2.2µs**
- Go stdlib solutions (map+RWMutex, sync.Map) vary widely
- Go's feature-rich DI libraries (samber/do) have high overhead

---

---

## Node.js DI Libraries Compared

| Library | Version | Type | Description |
|---------|---------|------|-------------|
| **Manual DI** | - | Baseline | Direct object instantiation |
| **Map-based** | - | Runtime | JavaScript Map for storage |
| **inversify** | 7.10.8 | Runtime | Popular TypeScript DI with decorators |
| **awilix** | 12.0.5 | Runtime | Lightweight function-based DI |

---

## Node.js Benchmark Results

### 1. Singleton Resolution

| Library | Language | Time (ns) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **17-32** | **1.0x** |
| Node.js manual | Node.js | 136 | 4-8x |
| Node.js awilix | Node.js | 176 | 5-10x |
| Node.js Map | Node.js | 271 | 8-16x |
| Node.js inversify | Node.js | 1,829 | 57-107x |

### 2. Deep Dependency Chain (4 levels)

| Library | Language | Time (ns) | vs Fastest |
|---------|----------|-----------|------------|
| Node.js Map | Node.js | 12 | 1.0x |
| **Rust dependency-injector** | Rust | **16-17** | 1.3-1.4x |
| Node.js manual | Node.js | 53 | 4.4x |
| Node.js inversify | Node.js | 253 | 21x |
| Node.js awilix | Node.js | 285 | 24x |

### 3. Container Creation

| Library | Language | Time | vs Fastest |
|---------|----------|------|------------|
| **Rust dependency-injector** | Rust | 434-740 ns | 1.0x |
| Node.js Map | Node.js | 877 ns | 1.2-2.0x |
| Node.js manual | Node.js | 1,901 ns | 2.6-4.4x |
| Node.js awilix | Node.js | 139 µs | 188-320x |
| Node.js inversify | Node.js | 286 µs | 386-658x |

### 4. Mixed Workload (100 operations)

| Library | Language | Time (µs) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **2.2** | **1.0x** |
| Node.js Map | Node.js | 6.6 | 3.0x |
| Node.js manual | Node.js | 7.8 | 3.5x |
| Node.js inversify | Node.js | 15.5 | 7.0x |
| Node.js awilix | Node.js | 825 | 375x |

---

## Python DI Libraries Compared

| Library | Version | Type | Description |
|---------|---------|------|-------------|
| **Manual DI** | - | Baseline | Direct object instantiation |
| **Dict-based** | - | Runtime | Python dict for storage |
| **dependency-injector** | 4.48.3 | Runtime | Most popular Python DI (Cython-optimized) |
| **injector** | 0.23.0 | Runtime | Google's Python DI |
| **punq** | 0.7.0 | Runtime | Lightweight DI |

---

## Python Benchmark Results

### 1. Singleton Resolution

| Library | Language | Time (ns) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **17-32** | **1.0x** |
| Python manual | Python | 56 | 1.8-3.3x |
| Python dict | Python | 79 | 2.5-4.6x |
| Python dependency-injector | Python | 95 | 3-5.6x |
| Python punq | Python | 824 | 26-48x |
| Python injector (Google) | Python | 3,319 | **104-195x** |

### 2. Deep Dependency Chain (4 levels)

| Library | Language | Time (ns) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **16-17** | **1.0x** |
| Python manual | Python | 78 | 4.6-4.9x |
| Python dependency-injector | Python | 127 | 7.5-7.9x |
| Python dict | Python | 132 | 7.8-8.3x |
| Python punq | Python | 885 | 52-55x |
| Python injector (Google) | Python | 3,495 | **206-218x** |

### 3. Container Creation

| Library | Language | Time | vs Fastest |
|---------|----------|------|------------|
| Python dict | Python | 81 ns | 1.0x |
| **Rust dependency-injector** | Rust | 434-740 ns | 5-9x |
| Python manual | Python | 1,132 ns | 14x |
| Python punq | Python | 36 µs | 444x |
| Python injector (Google) | Python | 66 µs | 815x |
| Python dependency-injector | Python | 601 µs | 7,420x |

### 4. Mixed Workload (100 operations)

| Library | Language | Time (µs) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **2.2** | **1.0x** |
| Python manual | Python | 15.0 | 6.8x |
| Python dependency-injector | Python | 15.7 | 7.1x |
| Python dict | Python | 16.7 | 7.6x |
| Python punq | Python | 90.4 | 41x |
| Python injector (Google) | Python | 342.4 | **156x** |

---

## C# DI Libraries Compared

| Library | Version | Type | Description |
|---------|---------|------|-------------|
| **Manual DI** | - | Baseline | Direct object instantiation |
| **Dictionary-based** | - | Runtime | C# Dictionary for storage |
| **Microsoft.Extensions.DI** | 8.0 | Runtime | Built-in .NET DI framework |

---

## C# Benchmark Results

### 1. Singleton Resolution

| Library | Language | Time (ns) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **17-32** | **1.0x** |
| C# Dictionary | C# | 142 | 4-8x |
| C# MS.Extensions.DI | C# | 208 | 6-12x |
| C# Manual | C# | 393 | 12-23x |

### 2. Deep Dependency Chain (4 levels)

| Library | Language | Time (ns) | vs Fastest |
|---------|----------|-----------|------------|
| C# Manual | C# | 4 | 1.0x |
| **Rust dependency-injector** | Rust | **16-17** | 4x |
| C# Dictionary | C# | 64 | 16x |
| C# MS.Extensions.DI | C# | 237 | 59x |

### 3. Container Creation

| Library | Language | Time | vs Fastest |
|---------|----------|------|------------|
| C# Dictionary | C# | 203 ns | 1.0x |
| **Rust dependency-injector** | Rust | 434-740 ns | 2-4x |
| C# Manual | C# | 1,604 ns | 8x |
| C# MS.Extensions.DI | C# | 13,580 ns | 67x |

### 4. Mixed Workload (100 operations)

| Library | Language | Time (µs) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **2.2** | **1.0x** |
| C# Manual | C# | 3.4 | 1.5x |
| C# Dictionary | C# | 30.1 | 14x |
| C# MS.Extensions.DI | C# | 31.2 | 14x |

---

## Summary: Rust vs Go vs Node.js vs Python vs C# DI Performance

### Speed Comparison (Best per Language)

| Operation | Go | Node.js | Python | C# | Rust |
|-----------|-----|---------|--------|-----|------|
| Singleton lookup | 15 ns | 136 ns | 56 ns | 142 ns | **17-32 ns** |
| Dependency chain | 11 ns | 12 ns | 78 ns | 4 ns | **16-17 ns** |
| Container creation | 0.3 ns | 877 ns | 81 ns | 203 ns | 434-740 ns |
| Mixed workload (100 ops) | 7 µs | 6.6 µs | 15 µs | 3.4 µs | **2.2 µs** |

### Popular DI Library Comparison

| Operation | Go samber/do | Node.js inversify | Python dep-injector | C# MS.Extensions.DI | Rust dependency-injector |
|-----------|--------------|-------------------|---------------------|---------------------|--------------------------|
| Singleton lookup | 767 ns | 1,829 ns | 95 ns | 208 ns | **17-32 ns** |
| Dependency chain | 276 ns | 253 ns | 127 ns | 237 ns | **16-17 ns** |
| Container creation | 27 µs | 139 µs | 601 µs | 13.6 µs | 434-740 ns |
| Mixed workload (100 ops) | 125 µs | 15 µs | 15.7 µs | 31 µs | **2.2 µs** |

### Feature Comparison

| Feature | Go samber/do | Node.js inversify | Python dep-injector | C# MS.Extensions.DI | Rust dependency-injector |
|---------|--------------|-------------------|---------------------|---------------------|--------------------------|
| Singleton | ✅ | ✅ | ✅ | ✅ | ✅ |
| Transient | ✅ | ✅ | ✅ | ✅ | ✅ |
| Scoped | ✅ | ✅ | ✅ | ✅ | ✅ |
| Lazy | ✅ | ✅ | ✅ | ✅ | ✅ |
| Factory | ✅ | ✅ | ✅ | ✅ | ✅ |
| Named Services | ✅ | ✅ | ✅ | ✅ | ❌ |
| Decorators | ❌ | ✅ | ✅ | ✅ | ❌ |
| Async Support | ✅ | ✅ | ✅ | ✅ | ✅ |
| Zero Allocations | ❌ | ❌ | ❌ | ❌ | ✅ |
| Hot Cache | ❌ | ❌ | ❌ | ❌ | ✅ |
| Compile-time Safety | ❌ | ❌ | ❌ | ❌ | ✅ |
| Source Generator | ❌ | ❌ | ❌ | ✅ | ❌ |

---

## Conclusions

### Why Rust `dependency-injector` is Faster

1. **Zero allocations** - No heap allocation per resolution
2. **Thread-local hot cache** - Frequently accessed services cached locally
3. **Lock-free DashMap** - Concurrent reads without mutex contention
4. **No reflection** - All type resolution at compile time
5. **Inlined hot paths** - Critical code paths optimized by LLVM

### Performance Rankings

**Singleton Resolution:**
1. 🥇 Go sync.Map (15 ns)
2. 🥇 **Rust dependency-injector** (17-32 ns)
3. 🥉 Python manual (56 ns)
4. Python dependency-injector (95 ns)
5. Node.js manual (136 ns)
6. C# Dictionary (142 ns)
7. C# MS.Extensions.DI (208 ns)
8. Go samber/do (767 ns)
9. Node.js inversify (1,829 ns)
10. Python injector (3,319 ns)

**Mixed Workload (100 ops):**
1. 🥇 **Rust dependency-injector** (2.2 µs)
2. 🥈 C# Manual (3.4 µs)
3. 🥉 Node.js Map (6.6 µs)
4. Go map+RWMutex (7 µs)
5. Python manual (15.0 µs)
6. Python dependency-injector (15.7 µs)
7. C# Dictionary / MS.Extensions.DI (30-31 µs)
8. Go samber/do (125 µs)
9. Python injector (342 µs)

### When to Use Each

#### Rust `dependency-injector`
- **High-performance services** requiring sub-microsecond DI
- **Memory-constrained environments** (zero allocation per resolution)
- **Concurrent workloads** with many threads accessing the container
- **Type-safe applications** where compile-time guarantees matter

#### Go DI Libraries
- **sync.Map/map+RWMutex**: When you need maximum speed
- **samber/do**: When you need generics-based DI with good developer experience
- **uber/dig**: When you need advanced features like decoration and groups

#### Node.js DI Libraries
- **Manual/Map**: When you need maximum speed for simple use cases
- **inversify**: When you need TypeScript decorators and enterprise patterns
- **awilix**: When you need lightweight function-based DI

#### Python DI Libraries
- **Manual/Dict**: When you need maximum speed (~56ns)
- **dependency-injector**: Best balance of features and performance (~95ns, Cython-optimized)
- **punq**: Lightweight alternative with good performance (~824ns)
- **injector (Google)**: When you need advanced features but can accept slower resolution

#### C# DI Libraries
- **Manual/Dictionary**: When you need maximum speed in hot paths
- **MS.Extensions.DI**: Standard choice for ASP.NET Core applications (~208ns, full-featured)
- For high-performance scenarios, consider the Rust FFI bindings

---

## Reproducing Benchmarks

### Rust Benchmarks

```bash
cargo bench --bench container_bench
cargo bench --bench comparison_bench
```

### Go Benchmarks

```bash
cd benchmarks/go-comparison
go test -bench=. -benchmem -count=3
```

### Node.js Benchmarks

```bash
cd benchmarks/nodejs-comparison
pnpm install
pnpm bench
```

### Python Benchmarks

```bash
cd benchmarks/python-comparison
python -m venv .venv
source .venv/bin/activate
pip install dependency-injector injector punq
python benchmark.py
```

### C# Benchmarks

```bash
cd benchmarks/csharp-comparison
dotnet run -c Release
```

---

*Benchmarks run on Intel i9-13900K, Linux, .NET 8.0, Python 3.13.3, Node.js v22.13.1, Go 1.24, Rust 1.85, December 2025*

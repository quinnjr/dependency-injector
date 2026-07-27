# Dependency Injector: Cross-Language Benchmark Comparison

Comprehensive benchmarks comparing Rust `dependency-injector` against popular Go, Node.js, Python, and C# DI libraries.

**Test Environment:**
- Measured: **2026-07-27**
- CPU: Intel Core i9-13900K (32 threads)
- OS: Linux 7.1.4-arch1-1 (native Linux — **not** WSL2)
- Rust: 1.97.1 (release mode, criterion)
- Go: 1.26.5
- Node.js: v26.5.0
- Python: 3.14.6
- C#: **not re-measured** — no .NET SDK on this machine (see caveats below)

All suites were run sequentially on an otherwise idle machine, never in parallel.

---

## Reading these numbers

Three things you need to know before comparing anything in this document.

1. **The environment changed.** The previous revision of this document was measured
   under WSL2; everything here (except C#) was measured on native Linux with newer
   toolchains. **Figures are not comparable to the previous revision of this document.**
   Do not read a change between revisions as a performance regression or improvement in
   any library.

2. **The mixed-workload benchmark got more honest, so five figures went up.** The
   "5% scope creation" branch of the mixed workload used to be a no-op or a duplicate
   resolve for **samber/do, inversify, and all three Python libraries** (dependency-injector,
   injector, punq). It now performs real scope / child-container creation. Those five
   mixed-workload figures are therefore **higher** than in the previous revision. That is
   the fairness correction landing, not a regression in those libraries.

3. **Python's `dependency-injector` mixed-workload figure is dominated by container
   instantiation.** Instantiating its declarative container costs **93.60 µs**, and the
   corrected mixed workload does that five times per 100-operation iteration — which is
   essentially the entire 470.70 µs result. It is a statement about container construction
   cost, not about resolution cost (resolution is 56.05 ns).

**Also note:**

- **JavaScript numbers move run to run** (JIT warm-up and deopt). A second run of the same
  build gave inversify deep-chain **84.49 ns** vs the **42.34 ns** reported here, and
  inversify mixed workload **53.96 µs** vs **48.47 µs**. All Node figures below come from a
  single run so the columns are mutually consistent; treat inversify in particular as
  approximate to within roughly a factor of two.
- **`†` marks C# figures carried over from the earlier WSL2 run** (December 2025, .NET 8.0).
  They were **not** re-measured and are **not directly comparable** to the rest of this
  document.
- **`‡` marks the Go uber/dig concurrent-reads cell**, which was previously unmeasurable
  because of a bug in the benchmark itself. The bug was found and fixed in this revision;
  the cell now carries a real figure. See the footnote in that section.
- **`§` marks Rust rows not captured in the 2026-07-27 run.** The `comparison_bench`
  `container_creation` and `concurrent_reads` groups were not recorded in this measurement
  pass, so no current figure is published for them rather than reprinting a WSL2-era one.

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
| **Go manual** | Go | 0.1383 ns | 0 | 1.0x |
| **Go sync.Map** | Go | 8.28 ns | 0 | 59.9x |
| **Go map+RWMutex** | Go | 11.64 ns | 0 | 84.2x |
| **Go goioc/di** | Go | 61.05 ns | 0 | 441x |
| **Go samber/do** | Go | 199.9 ns | 6 | 1,445x |
| **Go uber/dig** | Go | 922.7 ns | 24 | 6,672x |
| | | | | |
| **Rust manual** | Rust | 7.94 ns | 0 | 57.4x |
| **Rust dependency-injector** | Rust | **9.30 ns** | 0 | 67.2x |
| **Rust shaku** | Rust | 19.85 ns | 0 | 143.5x |
| **Rust HashMap+RwLock** | Rust | 20.14 ns | 0 | 145.6x |
| **Rust DashMap** | Rust | 20.57 ns | 0 | 148.7x |

The `vs Fastest` baseline is Go's manual DI, which the compiler inlines into a direct
field access — it is a floor, not a container.

**Key Insights:**
- `dependency-injector` (9.30 ns) is the fastest *container* measured in this table, and
  sits within 1.4 ns of hand-written Rust manual DI (7.94 ns)
- Go's `sync.Map` (8.28 ns) is marginally faster than `dependency-injector`; the two are
  within about 1 ns of each other and effectively tied
- `dependency-injector` is roughly 2.2x faster than the naive Rust baselines it replaces
  (HashMap+RwLock 20.14 ns, DashMap 20.57 ns) and 2.1x faster than shaku (19.85 ns)
- Go's reflection-based libraries pay heavily: samber/do is 21x slower than
  `dependency-injector` with 6 allocations per resolve, and uber/dig is 99x slower with 24
- goioc/di stays allocation-free but is still 6.6x slower than `dependency-injector`

---

### 2. Deep Dependency Chain (Service with Dependencies)

Resolving a service that has multiple levels of dependencies.

| Library | Language | Time | Allocations |
|---------|----------|------|-------------|
| **Go manual** | Go | 0.1128 ns | 0 |
| **Go sync.Map** | Go | 9.51 ns | 0 |
| **Go map+RWMutex** | Go | 12.26 ns | 0 |
| **Go samber/do** | Go | 211.7 ns | 6 |
| **Go uber/dig** | Go | 832.7 ns | 24 |
| | | | |
| **Rust dependency-injector** | Rust | **9.23 ns** | 0 |
| **Rust HashMap+RwLock** | Rust | 20.21 ns | 0 |
| **Rust ferrous-di** | Rust | 23.16 ns | 0 |
| **Rust shaku** | Rust | 35.11 ns | 0 |

**Key Insights:**
- Dependency depth is free for `dependency-injector`: 9.23 ns for the deep chain versus
  9.30 ns for a single singleton, because the chain resolves to pre-cached singletons
- shaku does *not* have that property — it goes from 19.85 ns (singleton) to 35.11 ns
  (deep chain), a 1.8x increase as depth grows
- Go's `sync.Map` (9.51 ns) tracks `dependency-injector` closely here as well
- Go's reflection-based libraries stay expensive: samber/do 211.7 ns and uber/dig 832.7 ns,
  both still allocating on every resolve

---

### 3. Container Creation

Creating a new DI container instance.

| Library | Language | Time | Allocations |
|---------|----------|------|-------------|
| **Go sync.Map** | Go | 0.1201 ns | 0 |
| **Go manual** | Go | 0.7512 ns | 0 |
| **Go map+RWMutex** | Go | 5.81 ns | 0 |
| **Go samber/do** | Go | 2.32 µs | 30 |
| **Go uber/dig** | Go | 13.72 µs | 49 |
| | | | |
| **Rust (all crates)** | Rust | not re-measured § | — |

**Key Insights:**
- Go's stdlib containers are essentially free to create; the compiler reduces `sync.Map`
  and manual construction to near-zero cost
- The reflection-based Go libraries pay for their registration graph up front: samber/do
  2.32 µs with 30 allocations, uber/dig 13.72 µs with 49
- uber/dig costs ~5.9x what samber/do costs to build a container
- Container creation is normally a one-time startup cost, so these figures matter far less
  than resolution cost for long-lived services

§ The Rust `container_creation` group was not captured in the 2026-07-27 run. Rather than
reprint the WSL2-era figure as if it were current, no number is published here; re-run
`cargo bench --bench comparison_bench -- container_creation` to obtain one.

---

### 4. Concurrent Access (Parallel Reads)

Performance under concurrent read load (32 goroutines/threads).

| Library | Language | Time/op | Allocations | vs Fastest |
|---------|----------|---------|-------------|------------|
| **Go sync.Map** | Go | 0.5262 ns | — | 1.0x |
| **Go map+RWMutex** | Go | 43.32 ns | — | 82.3x |
| **Go uber/dig** ‡ | Go | 265.2 ns | 24 (768 B) | 504x |
| **Go samber/do** | Go | 348.2 ns | 6 (144 B) | 662x |
| | | | | |
| **Rust (all crates)** | Rust | not re-measured § | — | — |

‡ **Benchmark bug, found and fixed in this revision.** `BenchmarkConcurrentReads/uber_dig`
used to abort the entire test binary with `fatal error: concurrent map read and map write`.
The fault was in the benchmark, not in dig: dig memoizes a constructor's result into
container-internal maps on the *first* resolution, and the benchmark raced that first
`Invoke` across goroutines via `RunParallel`, so many goroutines wrote the same map at once.
Because that is a Go *fatal* runtime error it killed the whole binary rather than failing
one case. The fix resolves once before `RunParallel` so every parallel iteration takes the
memoized read path; the race detector fired on every run before the fix and is clean across
repeated `-race` runs after it. This also makes the cell apples-to-apples — every other
target in this benchmark registers before going parallel, so they all measure *warm*
concurrent reads, whereas dig was uniquely measuring cold-start-plus-race. **Caveat:** dig
still does not document `Invoke` as safe for concurrent use even when warm, so callers who
resolve lazily need their own synchronization.

§ The Rust `concurrent_reads` group was not captured in the 2026-07-27 run; no current
figure is published for it.

**Key Insights:**
- Go's `sync.Map` is in a class of its own for concurrent reads (0.5262 ns/op) — it is
  purpose-built for read-mostly workloads
- A plain `map` behind an `RWMutex` costs 82x more (43.32 ns) once 32 readers contend
- uber/dig is the second-slowest concurrent reader at 265.2 ns and allocates 24 times
  (768 B) on every read, even on the warm memoized path
- samber/do is the slowest at 348.2 ns, but with 6 allocations per read it is far lighter
  on the allocator than dig
- Both DI libraries are 500-660x off `sync.Map`; if concurrent read throughput is the
  constraint, the container is the wrong place to put the hot lookup

---

### 5. Mixed Workload (100 Operations)

Simulating realistic usage: 80% resolutions, 15% lookups, 5% scope creation.

| Library | Language | Time | Allocations |
|---------|----------|------|-------------|
| **Go sync.Map** | Go | 1.67 µs | 20 |
| **Go map+RWMutex** | Go | 1.96 µs | 20 |
| **Go samber/do** | Go | 29.98 µs | 715 |
| | | | |
| **Rust dependency-injector** | Rust | **1.60 µs** | 0 |
| **Rust shaku** | Rust | 1.85 µs | 0 |
| **Rust DashMap basic** | Rust | 5.44 µs | 0 |

**Key Insights:**
- **Rust `dependency-injector` is the fastest entry in this table at 1.60 µs**, ahead of
  shaku (1.85 µs) and both Go stdlib approaches
- The margin over Go's stdlib is small — `sync.Map` at 1.67 µs is only 4% behind — but the
  Go versions allocate 20 times per 100 operations while the Rust ones allocate zero
- samber/do costs 29.98 µs and 715 allocations, 18.7x slower than `dependency-injector`;
  this figure includes real scope creation for the first time (see
  [Reading these numbers](#reading-these-numbers))
- The naive Rust DashMap baseline (5.44 µs) is 3.4x slower than `dependency-injector`,
  which is the gap the crate's hot cache and inlined fast path are buying

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
| Node.js manual | Node.js | 3.29 | 1.0x |
| Node.js Map | Node.js | 7.34 | 2.2x |
| **Rust dependency-injector** | Rust | **9.30** | 2.8x |
| Node.js awilix | Node.js | 26.51 | 8.1x |
| Node.js inversify | Node.js | 57.90 | 17.6x |

`dependency-injector` is 2.9x faster than awilix and 6.2x faster than inversify, but the
V8-inlined manual and Map baselines beat it outright on this microbenchmark.

### 2. Deep Dependency Chain (4 levels)

| Library | Language | Time (ns) | vs Fastest |
|---------|----------|-----------|------------|
| Node.js manual | Node.js | 3.95 | 1.0x |
| Node.js Map | Node.js | 4.82 | 1.2x |
| **Rust dependency-injector** | Rust | **9.23** | 2.3x |
| Node.js awilix | Node.js | 32.71 | 8.3x |
| Node.js inversify | Node.js | 42.34 | 10.7x |

inversify's deep-chain figure is the least stable number in this document — a repeat run
of the same build produced 84.49 ns.

### 3. Container Creation

| Library | Language | Time | vs Fastest |
|---------|----------|------|------------|
| Node.js Map | Node.js | 30.71 ns | 1.0x |
| Node.js manual | Node.js | 31.24 ns | 1.0x |
| Node.js inversify | Node.js | 11.71 µs | 381x |
| Node.js awilix | Node.js | 13.99 µs | 456x |
| **Rust dependency-injector** | Rust | not re-measured § | — |

### 4. Mixed Workload (100 operations)

| Library | Language | Time (µs) | vs Fastest |
|---------|----------|-----------|------------|
| Node.js manual | Node.js | 0.14 | 1.0x |
| Node.js Map | Node.js | 0.26 | 1.9x |
| **Rust dependency-injector** | Rust | **1.60** | 11.4x |
| Node.js awilix | Node.js | 9.59 | 68.5x |
| Node.js inversify | Node.js | 48.47 | 346x |

The Node manual and Map mixed-workload figures work out to 1.4 ns and 2.6 ns *per
operation*, which is below the measured cost of a single Map lookup (7.34 ns) in section 1.
V8 is optimising part of the loop away, so treat those two cells as a lower bound rather
than a like-for-like comparison. inversify's 48.47 µs includes real scope creation for the
first time (see [Reading these numbers](#reading-these-numbers)).

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
| **Rust dependency-injector** | Rust | **9.30** | **1.0x** |
| Python manual | Python | 27.15 | 2.9x |
| Python dict | Python | 40.34 | 4.3x |
| Python dependency-injector | Python | 56.05 | 6.0x |
| Python punq | Python | 396.70 | 42.7x |
| Python injector (Google) | Python | 968.12 | **104x** |

### 2. Deep Dependency Chain (4 levels)

| Library | Language | Time (ns) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **9.23** | **1.0x** |
| Python manual | Python | 27.91 | 3.0x |
| Python dict | Python | 40.07 | 4.3x |
| Python dependency-injector | Python | 54.58 | 5.9x |
| Python punq | Python | 388.84 | 42.1x |
| Python injector (Google) | Python | 961.94 | **104x** |

### 3. Container Creation

| Library | Language | Time | vs Fastest |
|---------|----------|------|------------|
| Python dict | Python | 60.25 ns | 1.0x |
| Python manual | Python | 344.82 ns | 5.7x |
| Python injector (Google) | Python | 15.10 µs | 251x |
| Python punq | Python | 15.82 µs | 262x |
| Python dependency-injector | Python | 93.60 µs | **1,554x** |
| **Rust dependency-injector** | Rust | not re-measured § | — |

### 4. Mixed Workload (100 operations)

| Library | Language | Time (µs) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **1.60** | **1.0x** |
| Python dict | Python | 4.70 | 2.9x |
| Python manual | Python | 4.71 | 2.9x |
| Python punq | Python | 53.15 | 33.2x |
| Python injector (Google) | Python | 110.11 | 68.8x |
| Python dependency-injector | Python | 470.70 | **294x** |

Python `dependency-injector`'s 470.70 µs is almost entirely container construction: the
corrected 5% scope-creation branch instantiates its declarative container five times per
100-operation iteration at 93.60 µs each. Its *resolution* cost (56.05 ns) is the best of
the three Python libraries.

---

## C# DI Libraries Compared

> **† All C# figures in this section are from the earlier WSL2 run (December 2025,
> .NET 8.0).** They could not be re-measured on 2026-07-27
> because no .NET SDK is installed on this machine. They are **not directly comparable**
> to the Go, Node.js, Python and Rust figures elsewhere in this document, which were
> measured on native Linux with newer toolchains. The Rust rows in these tables *are*
> current, so every C#-vs-Rust ratio below spans two different environments — read them as
> indicative only.

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
| **Rust dependency-injector** | Rust | **9.30** | **1.0x** |
| C# Dictionary † | C# | 142 | 15.3x |
| C# MS.Extensions.DI † | C# | 208 | 22.4x |
| C# Manual † | C# | 393 | 42.3x |

### 2. Deep Dependency Chain (4 levels)

| Library | Language | Time (ns) | vs Fastest |
|---------|----------|-----------|------------|
| C# Manual † | C# | 4 | 1.0x |
| **Rust dependency-injector** | Rust | **9.23** | 2.3x |
| C# Dictionary † | C# | 64 | 16x |
| C# MS.Extensions.DI † | C# | 237 | 59x |

### 3. Container Creation

| Library | Language | Time | vs Fastest |
|---------|----------|------|------------|
| C# Dictionary † | C# | 203 ns | 1.0x |
| C# Manual † | C# | 1,604 ns | 7.9x |
| C# MS.Extensions.DI † | C# | 13,580 ns | 66.9x |
| **Rust dependency-injector** | Rust | not re-measured § | — |

### 4. Mixed Workload (100 operations)

| Library | Language | Time (µs) | vs Fastest |
|---------|----------|-----------|------------|
| **Rust dependency-injector** | Rust | **1.60** | **1.0x** |
| C# Manual † | C# | 3.4 | 2.1x |
| C# Dictionary † | C# | 30.1 | 18.8x |
| C# MS.Extensions.DI † | C# | 31.2 | 19.5x |

---

## Summary: Rust vs Go vs Node.js vs Python vs C# DI Performance

### Speed Comparison (Best per Language)

Fastest measured entry per language, with the approach named. Manual/stdlib baselines are
compiler- or JIT-inlined and are floors rather than containers.

| Operation | Go | Node.js | Python | C# † | Rust |
|-----------|-----|---------|--------|-----|------|
| Singleton lookup | 0.1383 ns (manual) | 3.29 ns (manual) | 27.15 ns (manual) | 142 ns (Dictionary) † | 7.94 ns (manual) / **9.30 ns (dependency-injector)** |
| Dependency chain | 0.1128 ns (manual) | 3.95 ns (manual) | 27.91 ns (manual) | 4 ns (Manual) † | 7.91 ns (manual) / **9.23 ns (dependency-injector)** |
| Container creation | 0.1201 ns (sync.Map) | 30.71 ns (Map) | 60.25 ns (dict) | 203 ns (Dictionary) † | not re-measured § |
| Mixed workload (100 ops) | 1.67 µs (sync.Map) | 0.14 µs (manual) ‖ | 4.70 µs (dict) | 3.4 µs (Manual) † | **1.60 µs (dependency-injector)** |

† C# column: earlier WSL2 run (.NET 8.0, December 2025), **not** re-measured and **not
directly comparable** to the other columns.
‖ The Node manual mixed-workload figure is partly optimised away by V8 — see the Node.js
mixed-workload note above.

### Popular DI Library Comparison

| Operation | Go samber/do | Node.js inversify | Python dep-injector | C# MS.Extensions.DI † | Rust dependency-injector |
|-----------|--------------|-------------------|---------------------|---------------------|--------------------------|
| Singleton lookup | 199.9 ns | 57.90 ns | 56.05 ns | 208 ns † | **9.30 ns** |
| Dependency chain | 211.7 ns | 42.34 ns | 54.58 ns | 237 ns † | **9.23 ns** |
| Container creation | 2.32 µs | 11.71 µs | 93.60 µs | 13.6 µs † | not re-measured § |
| Mixed workload (100 ops) | 29.98 µs | 48.47 µs | 470.70 µs | 31 µs † | **1.60 µs** |

† C# column: earlier WSL2 run (.NET 8.0, December 2025), **not** re-measured and **not
directly comparable** to the other columns.

The samber/do, inversify and Python dependency-injector mixed-workload figures include
real scope creation for the first time in this revision — see
[Reading these numbers](#reading-these-numbers).

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

### Why Rust `dependency-injector` is Fast

It is the fastest full DI container measured here, and within ~1.4 ns of hand-written
manual DI in Rust. The mechanisms:

1. **Zero allocations** - No heap allocation per resolution
2. **Thread-local hot cache** - Frequently accessed services cached locally
3. **Lock-free DashMap** - Concurrent reads without mutex contention
4. **No reflection** - All type resolution at compile time
5. **Inlined hot paths** - Critical code paths optimized by LLVM

It does *not* beat the inlined baselines on a singleton lookup — Go manual (0.1383 ns),
Node.js manual (3.29 ns), Node.js Map (7.34 ns), Rust manual (7.94 ns) and Go `sync.Map`
(8.28 ns) all resolve faster, and none of them is a container: they are direct field or
raw-map accesses with no lifetime management, scoping, or type registry in the path. Among
actual DI containers, `dependency-injector` is the fastest measured in every language here.

### Performance Rankings

**Singleton Resolution** (every measured entry, fastest first):

1. Go manual — 0.1383 ns *(baseline, inlined)*
2. Node.js manual — 3.29 ns *(baseline, inlined)*
3. Node.js Map — 7.34 ns
4. Rust manual — 7.94 ns *(baseline)*
5. Go sync.Map — 8.28 ns
6. **Rust dependency-injector — 9.30 ns** *(fastest container measured)*
7. Go map+RWMutex — 11.64 ns
8. Rust shaku — 19.85 ns
9. Rust HashMap+RwLock — 20.14 ns
10. Rust DashMap — 20.57 ns
11. Rust ferrous-di — 22.91 ns
12. Node.js awilix — 26.51 ns
13. Python manual — 27.15 ns
14. Python dict — 40.34 ns
15. Python dependency-injector — 56.05 ns
16. Node.js inversify — 57.90 ns
17. Go goioc/di — 61.05 ns
18. C# Dictionary † — 142 ns
19. Go samber/do — 199.9 ns
20. C# MS.Extensions.DI † — 208 ns
21. Python punq — 396.70 ns
22. Go uber/dig — 922.7 ns
23. Python injector — 968.12 ns

**Mixed Workload (100 ops):**

1. Node.js manual — 0.14 µs ‖
2. Node.js Map — 0.26 µs ‖
3. **Rust dependency-injector — 1.60 µs**
4. Go sync.Map — 1.67 µs
5. Rust shaku — 1.85 µs
6. Go map+RWMutex — 1.96 µs
7. Rust ferrous-di — 2.34 µs
8. C# Manual † — 3.4 µs
9. Python dict — 4.70 µs
10. Python manual — 4.71 µs
11. Rust DashMap — 5.44 µs
12. Node.js awilix — 9.59 µs
13. Go samber/do — 29.98 µs
14. C# Dictionary / MS.Extensions.DI † — 30.1 / 31.2 µs
15. Node.js inversify — 48.47 µs
16. Python punq — 53.15 µs
17. Python injector — 110.11 µs
18. Python dependency-injector — 470.70 µs

† WSL2-era figure, not re-measured.
‖ Partly optimised away by V8; treat as a lower bound.

### When to Use Each

#### Rust `dependency-injector`
- **High-performance services** requiring sub-microsecond DI
- **Memory-constrained environments** (zero allocation per resolution)
- **Concurrent workloads** with many threads accessing the container
- **Type-safe applications** where compile-time guarantees matter

#### Go DI Libraries
- **sync.Map/map+RWMutex**: When you need maximum speed — `sync.Map` resolves in 8.28 ns
  and is unmatched under concurrent reads (0.5262 ns/op)
- **goioc/di**: Allocation-free at 61.05 ns, a middle ground between stdlib and full DI
- **samber/do**: When you need generics-based DI (199.9 ns, 6 allocs/resolve)
- **uber/dig**: When you need decoration and groups — but note it is the slowest Go option
  measured on resolution (922.7 ns singleton, 832.7 ns deep chain), the most expensive to
  construct (13.72 µs), and the heaviest allocator under concurrent reads (24 allocs/read).
  It also does not document `Invoke` as safe for concurrent use, so lazy first-resolution
  from multiple goroutines needs external synchronization

#### Node.js DI Libraries
- **Manual/Map**: When you need maximum speed for simple use cases (3.29-7.34 ns)
- **awilix**: Lightweight function-based DI and the faster of the two libraries on
  resolution (26.51 ns) and mixed workload (9.59 µs)
- **inversify**: When you need TypeScript decorators and enterprise patterns — 57.90 ns
  per resolve, and the least run-to-run stable library measured

#### Python DI Libraries
- **Manual/Dict**: When you need maximum speed (~27-40 ns)
- **dependency-injector**: Fastest Python library for resolution (~56 ns, Cython-optimized),
  but by far the most expensive container to construct (93.60 µs) — build it once at startup
- **punq**: Lightweight alternative, ~397 ns per resolve and cheap to construct (15.82 µs)
- **injector (Google)**: Slowest resolution measured in Python (~968 ns); use it when you
  need its feature set and resolution cost is not on your hot path

#### C# DI Libraries †
- **Manual/Dictionary**: When you need maximum speed in hot paths
- **MS.Extensions.DI**: Standard choice for ASP.NET Core applications (~208 ns †,
  full-featured)
- For high-performance scenarios, consider the Rust FFI bindings
- † All C# guidance above rests on the December 2025 WSL2 run; re-measure before relying on it

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

The full suite runs unfiltered. (It did not before this revision —
`BenchmarkConcurrentReads/uber_dig` aborted the test binary until the benchmark's
first-resolution race was fixed; see the `‡` footnote in section 4.)

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

*Go, Node.js, Python and Rust benchmarks run 2026-07-27 on Intel i9-13900K, native Linux
7.1.4-arch1-1, Rust 1.97.1, Go 1.26.5, Node.js v26.5.0, Python 3.14.6.*
*C# figures (†) are from the earlier run: WSL2, .NET 8.0, December 2025 — not re-measured.*

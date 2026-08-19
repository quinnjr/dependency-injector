# dependency-injector

> v0.2.2 | December 2025

## Performance Summary

| Operation | Time | Status |
|-----------|------|--------|
| `get_singleton` | **~9.4 ns** | ✅ ~1ns from manual DI |
| `get_transient` | **~24 ns** | ✅ |
| `contains_check` | **~11 ns** | ✅ |
| `create_scope` | **~80 ns** | ✅ |
| `scope_pool_acquire` | **~56 ns** | ✅ |
| `frozen_contains` | **~4 ns** | ✅ Perfect hash |

### vs Other Approaches

| Approach | Singleton | Concurrent (4 threads) |
|----------|-----------|------------------------|
| Manual DI | 7.94 ns | N/A |
| **dependency-injector** | **9.30 ns** | 90 µs |
| HashMap + RwLock | 20.14 ns | 93 µs |
| DashMap (basic) | 20.57 ns | 89 µs |

### vs Other Languages

Measured 2026-07-27 on native Linux; see
[BENCHMARK_COMPARISON.md](BENCHMARK_COMPARISON.md) for the full environment and caveats.

| Language | Library | Singleton | Mixed Workload |
|----------|---------|-----------|----------------|
| **Rust** | **dependency-injector** | **9.30 ns** | **1.60 µs** |
| Go | samber/do | 199.9 ns | 29.98 µs |
| C# † | MS.Extensions.DI | 208 ns | 31 µs |
| Python | dependency-injector | 56.05 ns | 470.70 µs |
| Node.js | inversify | 57.90 ns | 48.47 µs |

† C# was not re-measured (no .NET SDK on this machine); those figures are from the earlier
WSL2 run and are not directly comparable.

---

## Future Optimizations

### Profile-Guided Optimization (PGO)

Build with PGO for 5-15% improvement:

```bash
RUSTFLAGS="-Cprofile-generate=/tmp/pgo" cargo build --release
./target/release/bench
RUSTFLAGS="-Cprofile-use=/tmp/pgo" cargo build --release
```

**Expected:** 9.4ns → ~8.0ns

---

## Quality Assurance

### Memory: ✅ Zero Leaks

| Tool | Status |
|------|--------|
| dhat | ✅ 0 leaks, 51,800 allocs properly freed |
| Valgrind | ✅ 0 definitely/indirectly/possibly lost |

### Fuzzing: ✅ Passing

All fuzz targets passing (1M+ iterations):
- `fuzz_container` - Basic operations
- `fuzz_scoped` - Hierarchical scopes
- `fuzz_concurrent` - Multi-threaded access
- `fuzz_lifecycle` - Lazy/transient/locking

---

## Commands

```bash
# Benchmarks
cargo bench                                    # All benchmarks
cargo bench --bench comparison_bench           # vs other Rust DI crates

# Profiling
cargo run --example memory_profiler --features dhat-heap --release
valgrind --leak-check=full ./target/profiling/examples/memory_profiler

# Fuzzing
cd fuzz && cargo +nightly fuzz run fuzz_container -- -max_total_time=60
```

---

*See [CHANGELOG.md](CHANGELOG.md) for version history*
*See [BENCHMARK_COMPARISON.md](BENCHMARK_COMPARISON.md) for cross-language benchmarks*
*See [RUST_DI_COMPARISON.md](RUST_DI_COMPARISON.md) for Rust ecosystem comparison*

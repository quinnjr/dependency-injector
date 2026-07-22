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
| Manual DI | 8.4 ns | N/A |
| **dependency-injector** | **9.4 ns** | 90 µs |
| HashMap + RwLock | 21.5 ns | 93 µs |
| DashMap (basic) | 22.2 ns | 89 µs |

### vs Other Languages

| Language | Library | Singleton | Mixed Workload |
|----------|---------|-----------|----------------|
| **Rust** | **dependency-injector** | **17-32 ns** | **2.2 µs** |
| Go | samber/do | 767 ns | 125 µs |
| C# | MS.Extensions.DI | 208 ns | 31 µs |
| Python | dependency-injector | 95 ns | 15.7 µs |
| Node.js | inversify | 1,829 ns | 15 µs |

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

## Benchmarks

- [ ] Re-run the cross-language mixed-workload benchmarks (samber/do,
      inversify, dependency-injector/injector/punq) and update
      BENCHMARK_COMPARISON.md — the 5% scope-creation branches were fixed
      on 2026-07-21 to do real scope work, so the published numbers for
      those libraries are stale.

import { Injectable, computed, signal } from '@angular/core';

export interface BenchmarkEntry {
  commit: {
    id: string;
    message: string;
    timestamp: string;
    url: string;
    author: {
      name: string;
      username: string;
    };
  };
  date: number;
  tool: string;
  benches: Bench[];
}

export interface Bench {
  name: string;
  value: number;
  unit: string;
  range?: string;
  extra?: string;
}

export interface BenchmarkData {
  lastUpdate: string;
  repoUrl: string;
  entries: {
    [key: string]: BenchmarkEntry[];
  };
}

export interface ProcessedBenchmark {
  /** Benchmark suite this entry belongs to (bench names repeat across suites). */
  suite: string;
  name: string;
  current: number;
  previous: number | null;
  change: number | null;
  changePercent: number | null;
  unit: string;
  range?: string;
  history: { date: Date; value: number; commit: string }[];
}

@Injectable({
  providedIn: 'root'
})
export class BenchmarkService {
  private readonly DATA_URL = 'https://quinnjr.github.io/dependency-injector/benchmarks/data.js';
  private readonly FALLBACK_URL = './assets/benchmarks/data.json';

  readonly loading = signal(true);
  readonly error = signal<string | null>(null);
  readonly usingSampleData = signal(false);
  readonly data = signal<BenchmarkData | null>(null);
  readonly processedBenchmarks = signal<ProcessedBenchmark[]>([]);

  /** Short id of the most recent benchmarked commit across all suites. */
  readonly latestCommit = computed(() => {
    const data = this.data();
    if (!data?.entries) return '';

    const entries = Object.values(data.entries).flat();
    if (entries.length === 0) return '';

    const latest = [...entries].sort((a, b) => b.date - a.date)[0];
    return latest.commit.id.slice(0, 7);
  });

  async loadBenchmarks(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    this.usingSampleData.set(false);

    try {
      // Try to fetch from gh-pages benchmark data
      const response = await this.fetchBenchmarkData();

      if (response) {
        this.data.set(response);
        this.processedBenchmarks.set(this.processBenchmarks(response));
      } else {
        // Live data unavailable - fall back to bundled sample data
        this.useSampleData();
      }
    } catch (e) {
      console.error('Failed to load benchmarks:', e);
      this.error.set('Failed to load benchmark data. Showing sample data.');
      this.useSampleData();
    } finally {
      this.loading.set(false);
    }
  }

  /** Fall back to the bundled sample dataset, flagging it as such. */
  private useSampleData(): void {
    this.usingSampleData.set(true);
    const sampleData = this.getSampleData();
    this.data.set(sampleData);
    this.processedBenchmarks.set(this.processBenchmarks(sampleData));
  }

  private async fetchBenchmarkData(): Promise<BenchmarkData | null> {
    try {
      // The benchmark action stores data in a JS file with window.BENCHMARK_DATA = {...}
      const response = await fetch(this.DATA_URL);
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }

      const text = await response.text();
      // Parse the JS file to extract the data
      const match = text.match(/window\.BENCHMARK_DATA\s*=\s*(\{[\s\S]*\})/);
      if (match) {
        return JSON.parse(match[1]);
      }

      // Try parsing as plain JSON
      return JSON.parse(text);
    } catch (e) {
      console.warn('Benchmark live-data fetch failed, trying fallback:', e);
      // Try fallback URL
      try {
        const fallbackResponse = await fetch(this.FALLBACK_URL);
        if (fallbackResponse.ok) {
          return await fallbackResponse.json();
        }
      } catch (fallbackError) {
        console.warn('Benchmark fallback fetch failed:', fallbackError);
      }
      return null;
    }
  }

  private processBenchmarks(data: BenchmarkData): ProcessedBenchmark[] {
    const results: ProcessedBenchmark[] = [];

    for (const [suiteName, entries] of Object.entries(data.entries)) {
      if (!entries || entries.length === 0) continue;

      // Sort entries by date (newest first)
      const sortedEntries = [...entries].sort((a, b) => b.date - a.date);
      const latestEntry = sortedEntries[0];
      const previousEntry = sortedEntries[1] || null;

      for (const bench of latestEntry.benches) {
        const previousBench = previousEntry?.benches.find(b => b.name === bench.name);
        const change = previousBench ? bench.value - previousBench.value : null;
        const changePercent = previousBench && previousBench.value > 0
          ? ((bench.value - previousBench.value) / previousBench.value) * 100
          : null;

        // Build history for this benchmark
        const history = sortedEntries
          .filter(entry => entry.benches.some(b => b.name === bench.name))
          .map(entry => {
            const b = entry.benches.find(b => b.name === bench.name)!;
            return {
              date: new Date(entry.date),
              value: b.value,
              commit: entry.commit.id.slice(0, 7)
            };
          })
          .reverse(); // Oldest first for charts

        results.push({
          suite: suiteName,
          name: bench.name,
          current: bench.value,
          previous: previousBench?.value || null,
          change,
          changePercent,
          unit: bench.unit,
          range: bench.range,
          history
        });
      }
    }

    return results;
  }

  private getSampleData(): BenchmarkData {
    // Bundled sample results, measured on v0.2.1 (Dec 2025) - not live data.
    // Timestamps are fixed to the real measurement date, not the current time.
    const measured = Date.UTC(2025, 11, 15);
    const day = 24 * 60 * 60 * 1000;

    // Phase 11: Fast bit-mixing hash + single DashMap lookup + reduced shards
    return {
      lastUpdate: new Date(measured).toISOString(),
      repoUrl: 'https://github.com/quinnjr/dependency-injector',
      entries: {
        'Rust Benchmarks': [
          {
            commit: {
              id: 'e7ed62b',
              message: 'perf: Phase 11 - Fast bit-mixing hash + single DashMap lookup',
              timestamp: new Date(measured).toISOString(),
              url: 'https://github.com/quinnjr/dependency-injector/commit/e7ed62b',
              author: { name: 'Developer', username: 'quinnjr' }
            },
            date: measured,
            tool: 'cargo',
            benches: [
              // Registration benchmarks
              { name: 'registration/singleton_small', value: 123, unit: 'ns/iter', range: '± 3' },
              { name: 'registration/singleton_medium', value: 139, unit: 'ns/iter', range: '± 4' },
              { name: 'registration/lazy', value: 127, unit: 'ns/iter', range: '± 3' },
              { name: 'registration/transient', value: 128, unit: 'ns/iter', range: '± 3' },
              // Resolution benchmarks (Phase 11: ~9ns with fast hash)
              { name: 'resolution/get_singleton', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              { name: 'resolution/get_singleton_hot', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              { name: 'resolution/get_medium', value: 9.2, unit: 'ns/iter', range: '± 0.2' },
              { name: 'resolution/contains_check', value: 10.5, unit: 'ns/iter', range: '± 0.2' },
              { name: 'resolution/try_get_found', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              { name: 'resolution/try_get_not_found', value: 12.0, unit: 'ns/iter', range: '± 0.3' },
              // Transient benchmarks
              { name: 'transient/get_transient', value: 24.0, unit: 'ns/iter', range: '± 0.5' },
              // Scoped benchmarks (Phase 11: 4-shard DashMap for child scopes)
              { name: 'scoped/create_scope', value: 80, unit: 'ns/iter', range: '± 3' },
              { name: 'scoped/scope_pool_acquire', value: 56, unit: 'ns/iter', range: '± 2' },
              { name: 'scoped/resolve_from_parent', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              { name: 'scoped/deep_parent_chain', value: 18.0, unit: 'ns/iter', range: '± 0.4' },
              { name: 'scoped/resolve_override', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              // Batch registration
              { name: 'batch/fluent_4_services', value: 241, unit: 'ns/iter', range: '± 5' },
              { name: 'batch/closure_4_services', value: 340, unit: 'ns/iter', range: '± 6' },
              // Perfect hash benchmarks
              { name: 'perfect_hash/frozen_resolve', value: 14.5, unit: 'ns/iter', range: '± 0.3' },
              { name: 'perfect_hash/frozen_contains', value: 3.9, unit: 'ns/iter', range: '± 0.1' },
              // Concurrent benchmarks
              { name: 'concurrent/concurrent_reads_4', value: 90000, unit: 'ns/iter', range: '± 5000' },
              // Comparison benchmarks
              { name: 'comparison/singleton_resolution', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              { name: 'comparison/deep_dependency_chain', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              { name: 'comparison/container_creation', value: 80, unit: 'ns/iter', range: '± 3' },
              // Service scaling (O(1) lookup)
              { name: 'scaling/10_services', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              { name: 'scaling/50_services', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              { name: 'scaling/100_services', value: 9.0, unit: 'ns/iter', range: '± 0.2' },
              { name: 'scaling/500_services', value: 9.1, unit: 'ns/iter', range: '± 0.3' }
            ]
          },
          {
            commit: {
              id: 'v0.1.5',
              message: 'feat: Phase 4 - Derive macros for automatic injection',
              timestamp: new Date(measured - day).toISOString(),
              url: 'https://github.com/quinnjr/dependency-injector/commit/v0.1.5',
              author: { name: 'Developer', username: 'quinnjr' }
            },
            date: measured - day,
            tool: 'cargo',
            benches: [
              { name: 'registration/singleton_small', value: 250, unit: 'ns/iter', range: '± 5' },
              { name: 'registration/singleton_medium', value: 255, unit: 'ns/iter', range: '± 6' },
              { name: 'registration/lazy', value: 252, unit: 'ns/iter', range: '± 5' },
              { name: 'registration/transient', value: 251, unit: 'ns/iter', range: '± 5' },
              { name: 'resolution/get_singleton', value: 16.5, unit: 'ns/iter', range: '± 0.4' },
              { name: 'resolution/get_medium', value: 16.6, unit: 'ns/iter', range: '± 0.4' },
              { name: 'resolution/contains_check', value: 10.5, unit: 'ns/iter', range: '± 0.2' },
              { name: 'resolution/try_get_found', value: 16.5, unit: 'ns/iter', range: '± 0.4' },
              { name: 'resolution/try_get_not_found', value: 8.8, unit: 'ns/iter', range: '± 0.2' },
              { name: 'transient/get_transient', value: 23.5, unit: 'ns/iter', range: '± 0.5' },
              { name: 'scoped/create_scope', value: 100, unit: 'ns/iter', range: '± 3' },
              { name: 'scoped/resolve_from_parent', value: 16.8, unit: 'ns/iter', range: '± 0.4' },
              { name: 'scoped/resolve_override', value: 16.5, unit: 'ns/iter', range: '± 0.4' },
              { name: 'concurrent/concurrent_reads_4', value: 90000, unit: 'ns/iter', range: '± 5000' },
              { name: 'comparison/singleton_resolution', value: 16.5, unit: 'ns/iter', range: '± 0.4' },
              { name: 'comparison/deep_dependency_chain', value: 16.2, unit: 'ns/iter', range: '± 0.3' },
              { name: 'comparison/container_creation', value: 185, unit: 'ns/iter', range: '± 4' },
              { name: 'scaling/10_services', value: 16.6, unit: 'ns/iter', range: '± 0.3' },
              { name: 'scaling/50_services', value: 16.5, unit: 'ns/iter', range: '± 0.3' },
              { name: 'scaling/100_services', value: 16.6, unit: 'ns/iter', range: '± 0.4' },
              { name: 'scaling/500_services', value: 16.7, unit: 'ns/iter', range: '± 0.4' }
            ]
          },
          {
            commit: {
              id: 'v0.1.0',
              message: 'Initial release',
              timestamp: new Date(measured - 2 * day).toISOString(),
              url: 'https://github.com/quinnjr/dependency-injector/releases/tag/v0.1.0',
              author: { name: 'Developer', username: 'quinnjr' }
            },
            date: measured - 2 * day,
            tool: 'cargo',
            benches: [
              { name: 'registration/singleton_small', value: 854, unit: 'ns/iter', range: '± 24' },
              { name: 'registration/singleton_medium', value: 914, unit: 'ns/iter', range: '± 45' },
              { name: 'registration/lazy', value: 867, unit: 'ns/iter', range: '± 35' },
              { name: 'registration/transient', value: 858, unit: 'ns/iter', range: '± 38' },
              { name: 'resolution/get_singleton', value: 19.6, unit: 'ns/iter', range: '± 1.6' },
              { name: 'resolution/get_medium', value: 19.2, unit: 'ns/iter', range: '± 0.5' },
              { name: 'resolution/contains_check', value: 18.6, unit: 'ns/iter', range: '± 0.8' },
              { name: 'resolution/try_get_found', value: 19.4, unit: 'ns/iter', range: '± 0.5' },
              { name: 'resolution/try_get_not_found', value: 13.9, unit: 'ns/iter', range: '± 0.8' },
              { name: 'transient/get_transient', value: 26.9, unit: 'ns/iter', range: '± 1.4' },
              { name: 'scoped/create_scope', value: 870, unit: 'ns/iter', range: '± 34' },
              { name: 'scoped/resolve_from_parent', value: 38.4, unit: 'ns/iter', range: '± 1.7' },
              { name: 'scoped/resolve_override', value: 19.4, unit: 'ns/iter', range: '± 0.9' },
              { name: 'concurrent/concurrent_reads_4', value: 135000, unit: 'ns/iter', range: '± 6500' },
              { name: 'comparison/singleton_resolution', value: 19.2, unit: 'ns/iter', range: '± 0.5' },
              { name: 'comparison/deep_dependency_chain', value: 18.3, unit: 'ns/iter', range: '± 0.4' },
              { name: 'comparison/container_creation', value: 788, unit: 'ns/iter', range: '± 34' },
              { name: 'scaling/10_services', value: 19.9, unit: 'ns/iter', range: '± 0.25' },
              { name: 'scaling/50_services', value: 18.9, unit: 'ns/iter', range: '± 0.44' },
              { name: 'scaling/100_services', value: 18.6, unit: 'ns/iter', range: '± 0.61' },
              { name: 'scaling/500_services', value: 18.8, unit: 'ns/iter', range: '± 0.96' }
            ]
          }
        ]
      }
    };
  }

  formatValue(value: number, unit: string): string {
    if (unit.includes('ns')) {
      if (value >= 1_000_000) {
        return `${(value / 1_000_000).toFixed(2)} ms`;
      } else if (value >= 1_000) {
        return `${(value / 1_000).toFixed(2)} µs`;
      }
      return `${value.toFixed(2)} ns`;
    }
    return `${value.toFixed(2)} ${unit}`;
  }

  getChangeClass(changePercent: number | null): string {
    if (changePercent === null) return '';
    if (changePercent <= -5) return 'text-green-400'; // Faster is better
    if (changePercent >= 5) return 'text-red-400';    // Slower is worse
    return 'text-slate-400';
  }

  getChangeIcon(changePercent: number | null): string {
    if (changePercent === null) return '';
    if (changePercent <= -5) return '↓'; // Faster
    if (changePercent >= 5) return '↑';  // Slower
    return '→';
  }
}


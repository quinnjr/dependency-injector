import { Component, OnInit, computed, inject } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FontAwesomeModule } from '@fortawesome/angular-fontawesome';
import { BenchmarkService, ProcessedBenchmark } from '../../services/benchmark.service';
import { SeoService } from '../../services/seo.service';

@Component({
  selector: 'app-benchmarks',
  standalone: true,
  imports: [CommonModule, FontAwesomeModule],
  templateUrl: './benchmarks.html',
  styleUrl: './benchmarks.scss'
})
export class BenchmarksPage implements OnInit {
  readonly benchmarkService = inject(BenchmarkService);
  private readonly seo = inject(SeoService);

  readonly lastCommit = this.benchmarkService.latestCommit;

  readonly lastUpdate = computed(() => {
    const data = this.benchmarkService.data();
    if (!data?.lastUpdate) return '';

    return new Date(data.lastUpdate).toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    });
  });

  readonly sparklinePoints = computed(() => {
    const points = new Map<string, string>();
    for (const benchmark of this.benchmarkService.processedBenchmarks()) {
      // Composite key: bench names can repeat across suites
      points.set(`${benchmark.suite}/${benchmark.name}`, this.computeSparklinePoints(benchmark));
    }
    return points;
  });

  ngOnInit(): void {
    this.seo.setBenchmarksSeo();
    this.benchmarkService.loadBenchmarks();
  }

  formatValue(value: number, unit: string): string {
    return this.benchmarkService.formatValue(value, unit);
  }

  getChangeClass(changePercent: number | null): string {
    return this.benchmarkService.getChangeClass(changePercent);
  }

  getChangeIcon(changePercent: number | null): string {
    return this.benchmarkService.getChangeIcon(changePercent);
  }

  formatChange(changePercent: number | null): string {
    if (changePercent === null) return 'N/A';
    const sign = changePercent > 0 ? '+' : '';
    return `${sign}${changePercent.toFixed(1)}%`;
  }

  private computeSparklinePoints(benchmark: ProcessedBenchmark): string {
    if (!benchmark.history || benchmark.history.length < 2) return '';

    const values = benchmark.history.map(h => h.value);
    const min = Math.min(...values);
    const max = Math.max(...values);
    const range = max - min || 1;

    const width = 100;
    const height = 30;
    const padding = 2;

    return benchmark.history
      .map((h, i) => {
        const x = padding + (i / (benchmark.history.length - 1)) * (width - 2 * padding);
        const y = height - padding - ((h.value - min) / range) * (height - 2 * padding);
        return `${x},${y}`;
      })
      .join(' ');
  }
}

using System;
using System.Collections.Generic;
using System.Diagnostics;
using Microsoft.Extensions.DependencyInjection;

/// <summary>
/// C# DI Library Benchmark Comparison
///
/// Compares:
/// - Manual DI (baseline)
/// - Dictionary-based DI (simple runtime)
/// - Microsoft.Extensions.DependencyInjection (built-in .NET DI)
/// </summary>

// =============================================================================
// Test Services
// =============================================================================

public class Config
{
    public string DatabaseUrl { get; set; } = "postgres://localhost/test";
    public int MaxConnections { get; set; } = 10;
}

public class Database
{
    public Config Config { get; }
    public Database(Config config) => Config = config;
}

public class UserRepository
{
    public Database Db { get; }
    public bool CacheEnabled { get; set; } = true;
    public UserRepository(Database db) => Db = db;
}

public class UserService
{
    public UserRepository Repo { get; }
    public string Name { get; set; } = "UserService";
    public UserService(UserRepository repo) => Repo = repo;
}

// =============================================================================
// Manual DI (Baseline)
// =============================================================================

public class ManualContainer
{
    private readonly Config _config;
    private readonly Database _database;
    private readonly UserRepository _userRepo;
    private readonly UserService _userService;

    public ManualContainer()
    {
        _config = new Config();
        _database = new Database(_config);
        _userRepo = new UserRepository(_database);
        _userService = new UserService(_userRepo);
    }

    public Config GetConfig() => _config;
    public Database GetDatabase() => _database;
    public UserService GetUserService() => _userService;
}

// =============================================================================
// Dictionary-based DI (Simple runtime)
// =============================================================================

public class DictContainer
{
    private readonly Dictionary<string, object> _services = new();

    public void Register<T>(T service) where T : class
    {
        _services[typeof(T).FullName!] = service;
    }

    public T? Get<T>() where T : class
    {
        return _services.TryGetValue(typeof(T).FullName!, out var service) ? (T)service : null;
    }
}

// =============================================================================
// Benchmark utilities
// =============================================================================

public class BenchmarkResult
{
    public string Name { get; set; } = "";
    public double OpsPerSec { get; set; }
    public double AvgNs { get; set; }
}

public static class Benchmark
{
    // When DI_BENCH_SMOKE is set to a non-empty value, every benchmark runs a
    // token number of iterations. The resulting timings are meaningless as
    // measurements -- the point is to execute every code path cheaply so CI can
    // catch crashes, build errors and API drift without spending minutes.
    private static readonly bool Smoke =
        !string.IsNullOrEmpty(Environment.GetEnvironmentVariable("DI_BENCH_SMOKE"));
    private const int SmokeIterations = 2;
    private const int SmokeWarmup = 1;

    public static BenchmarkResult Run(string name, Action action, int iterations = 100000)
    {
        var warmup = Smoke ? SmokeWarmup : 1000;
        if (Smoke)
        {
            iterations = SmokeIterations;
        }

        // Warm up
        for (int i = 0; i < warmup; i++)
        {
            action();
        }

        // Force GC before benchmark
        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();

        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iterations; i++)
        {
            action();
        }
        sw.Stop();

        var totalNs = sw.Elapsed.TotalNanoseconds;
        var avgNs = totalNs / iterations;
        var opsPerSec = 1e9 / avgNs;

        return new BenchmarkResult
        {
            Name = name,
            OpsPerSec = opsPerSec,
            AvgNs = avgNs
        };
    }

    public static void PrintTable(List<BenchmarkResult> results, string timeUnit = "ns")
    {
        Console.WriteLine($"{"Library",-30} {"ops/sec",15} {"avg (" + timeUnit + ")",15}");
        Console.WriteLine(new string('-', 62));
        foreach (var r in results)
        {
            var timeVal = timeUnit switch
            {
                "µs" => $"{r.AvgNs / 1000:F2}",
                _ => $"{r.AvgNs:F2}"
            };
            Console.WriteLine($"{r.Name,-30} {r.OpsPerSec,15:N0} {timeVal,15}");
        }
        Console.WriteLine();
    }
}

// =============================================================================
// Main Program
// =============================================================================

public class Program
{
    public static void Main(string[] args)
    {
        Console.WriteLine("C# DI Library Benchmark");
        Console.WriteLine("=======================\n");
        Console.WriteLine($".NET: {Environment.Version}");
        Console.WriteLine($"OS: {Environment.OSVersion}");
        Console.WriteLine();

        // =====================================================================
        // Benchmark 1: Singleton Resolution
        // =====================================================================

        Console.WriteLine("1. Singleton Resolution");
        Console.WriteLine("-----------------------");

        // Setup containers
        var manual = new ManualContainer();

        var dictContainer = new DictContainer();
        dictContainer.Register(new Config());

        var msdiServices = new ServiceCollection();
        msdiServices.AddSingleton<Config>();
        msdiServices.AddSingleton<Database>();
        msdiServices.AddSingleton<UserRepository>();
        msdiServices.AddSingleton<UserService>();
        var msdiProvider = msdiServices.BuildServiceProvider();

        // Warm up MSDI
        _ = msdiProvider.GetService<Config>();

        var singletonResults = new List<BenchmarkResult>
        {
            Benchmark.Run("manual_di", () => { _ = manual.GetConfig(); }),
            Benchmark.Run("dict_based", () => { _ = dictContainer.Get<Config>(); }),
            Benchmark.Run("MS.Extensions.DI", () => { _ = msdiProvider.GetService<Config>(); }),
        };

        Benchmark.PrintTable(singletonResults, "ns");

        // =====================================================================
        // Benchmark 2: Deep Dependency Chain
        // =====================================================================

        Console.WriteLine("2. Deep Dependency Chain (4 levels)");
        Console.WriteLine("------------------------------------");

        // Setup dict with full chain
        var config = new Config();
        var db = new Database(config);
        var repo = new UserRepository(db);
        var svc = new UserService(repo);
        dictContainer.Register(svc);

        // Warm up
        _ = manual.GetUserService();
        _ = msdiProvider.GetService<UserService>();

        var deepResults = new List<BenchmarkResult>
        {
            Benchmark.Run("manual_di", () => { _ = manual.GetUserService(); }),
            Benchmark.Run("dict_based", () => { _ = dictContainer.Get<UserService>(); }),
            Benchmark.Run("MS.Extensions.DI", () => { _ = msdiProvider.GetService<UserService>(); }),
        };

        Benchmark.PrintTable(deepResults, "ns");

        // =====================================================================
        // Benchmark 3: Container Creation
        // =====================================================================

        Console.WriteLine("3. Container Creation");
        Console.WriteLine("---------------------");

        var creationResults = new List<BenchmarkResult>
        {
            Benchmark.Run("manual_di", () => { _ = new ManualContainer(); }, 10000),
            Benchmark.Run("dict_based", () =>
            {
                var c = new DictContainer();
                c.Register(new Config());
            }, 10000),
            Benchmark.Run("MS.Extensions.DI", () =>
            {
                var services = new ServiceCollection();
                services.AddSingleton<Config>();
                services.AddSingleton<Database>();
                services.AddSingleton<UserRepository>();
                services.AddSingleton<UserService>();
                var provider = services.BuildServiceProvider();
            }, 1000),
        };

        Benchmark.PrintTable(creationResults, "ns");

        // =====================================================================
        // Benchmark 4: Mixed Workload (100 operations)
        // =====================================================================

        Console.WriteLine("4. Mixed Workload (100 operations per iteration)");
        Console.WriteLine("------------------------------------------------");

        var mixedResults = new List<BenchmarkResult>
        {
            Benchmark.Run("manual_di", () =>
            {
                for (int i = 0; i < 100; i++)
                {
                    var op = i % 20;
                    if (op < 16)
                        _ = manual.GetConfig();
                    else if (op < 19)
                        _ = manual.GetDatabase();
                    else
                        _ = new ManualContainer();
                }
            }, 10000),
            Benchmark.Run("dict_based", () =>
            {
                for (int i = 0; i < 100; i++)
                {
                    var op = i % 20;
                    if (op < 16)
                        _ = dictContainer.Get<Config>();
                    else if (op < 19)
                        _ = dictContainer.Get<Database>();
                    else
                    {
                        var scope = new DictContainer();
                        scope.Register(new Config());
                    }
                }
            }, 10000),
            Benchmark.Run("MS.Extensions.DI", () =>
            {
                for (int i = 0; i < 100; i++)
                {
                    var op = i % 20;
                    if (op < 16)
                        _ = msdiProvider.GetService<Config>();
                    else if (op < 19)
                        _ = msdiProvider.GetService<Database>();
                    else
                    {
                        var scope = msdiProvider.CreateScope();
                        _ = scope.ServiceProvider.GetService<Config>();
                    }
                }
            }, 10000),
        };

        Benchmark.PrintTable(mixedResults, "µs");

        // =====================================================================
        // Summary
        // =====================================================================

        Console.WriteLine("============================");
        Console.WriteLine("Summary");
        Console.WriteLine("============================\n");

        Console.WriteLine("For comparison with Rust dependency-injector:");
        Console.WriteLine("- Rust singleton resolution: ~17-32 ns");
        Console.WriteLine("- Rust mixed workload (100 ops): ~2.2 µs");
        Console.WriteLine();
        Console.WriteLine("Best C# times from this benchmark:");
        Console.WriteLine($"- Singleton resolution: {singletonResults[0].AvgNs:F0} ns (manual_di)");
        Console.WriteLine($"- Mixed workload: {mixedResults[0].AvgNs / 1000:F2} µs (manual_di)");
    }
}

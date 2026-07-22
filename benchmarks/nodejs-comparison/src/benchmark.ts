/**
 * Node.js DI Library Benchmark Comparison
 *
 * Compares:
 * - Manual DI (baseline)
 * - Map-based DI (simple runtime)
 * - inversify (popular TypeScript DI)
 * - awilix (lightweight function-based DI)
 */

import 'reflect-metadata';
import { Bench } from 'tinybench';

// Import DI libraries
import { Container as InversifyContainer, injectable as inversifyInjectable, inject as inversifyInject } from 'inversify';
import { createContainer, asClass, asValue, InjectionMode } from 'awilix';

// =============================================================================
// Test Services
// =============================================================================

// Plain classes for manual DI
class Config {
  databaseUrl = 'postgres://localhost/test';
  maxConnections = 10;
}

class Database {
  constructor(public config: Config) {}
}

class UserRepository {
  constructor(public database: Database, public cacheEnabled = true) {}
}

class UserService {
  constructor(public userRepository: UserRepository, public name = 'UserService') {}
}

// =============================================================================
// Manual DI (Baseline)
// =============================================================================

class ManualContainer {
  private config: Config;
  private database: Database;
  private userRepo: UserRepository;
  private userService: UserService;

  constructor() {
    this.config = new Config();
    this.database = new Database(this.config);
    this.userRepo = new UserRepository(this.database);
    this.userService = new UserService(this.userRepo);
  }

  getConfig(): Config {
    return this.config;
  }

  getDatabase(): Database {
    return this.database;
  }

  getUserService(): UserService {
    return this.userService;
  }
}

// =============================================================================
// Map-based DI (Simple runtime)
// =============================================================================

class MapContainer {
  private services = new Map<string, unknown>();

  register<T>(name: string, service: T): void {
    this.services.set(name, service);
  }

  get<T>(name: string): T | undefined {
    return this.services.get(name) as T | undefined;
  }
}

// =============================================================================
// inversify setup
// =============================================================================

const TYPES = {
  Config: Symbol.for('Config'),
  Database: Symbol.for('Database'),
  UserRepository: Symbol.for('UserRepository'),
  UserService: Symbol.for('UserService'),
};

@inversifyInjectable()
class InversifyConfig {
  databaseUrl = 'postgres://localhost/test';
  maxConnections = 10;
}

@inversifyInjectable()
class InversifyDatabase {
  constructor(@inversifyInject(TYPES.Config) public config: InversifyConfig) {}
}

@inversifyInjectable()
class InversifyUserRepository {
  constructor(@inversifyInject(TYPES.Database) public db: InversifyDatabase) {}
  cacheEnabled = true;
}

@inversifyInjectable()
class InversifyUserService {
  constructor(@inversifyInject(TYPES.UserRepository) public repo: InversifyUserRepository) {}
  name = 'UserService';
}

function createInversifyContainer(): InversifyContainer {
  const container = new InversifyContainer();
  container.bind(TYPES.Config).to(InversifyConfig).inSingletonScope();
  container.bind(TYPES.Database).to(InversifyDatabase).inSingletonScope();
  container.bind(TYPES.UserRepository).to(InversifyUserRepository).inSingletonScope();
  container.bind(TYPES.UserService).to(InversifyUserService).inSingletonScope();
  return container;
}

// =============================================================================
// awilix setup
// =============================================================================

interface AwilixCradle {
  config: Config;
  database: Database;
  userRepository: UserRepository;
  userService: UserService;
}

function createAwilixContainer() {
  const container = createContainer<AwilixCradle>({
    injectionMode: InjectionMode.CLASSIC,
  });

  container.register({
    config: asClass(Config).singleton(),
    database: asClass(Database).singleton(),
    userRepository: asClass(UserRepository).singleton(),
    userService: asClass(UserService).singleton(),
  });

  return container;
}

// =============================================================================
// Simple benchmark function
// =============================================================================

function benchmark(name: string, fn: () => void, iterations = 100000): { name: string; opsPerSec: number; avgNs: number } {
  // Warm up
  for (let i = 0; i < 1000; i++) {
    fn();
  }

  const start = process.hrtime.bigint();
  for (let i = 0; i < iterations; i++) {
    fn();
  }
  const end = process.hrtime.bigint();

  const totalNs = Number(end - start);
  const avgNs = totalNs / iterations;
  const opsPerSec = 1e9 / avgNs;

  return { name, opsPerSec, avgNs };
}

// =============================================================================
// Benchmark Runner
// =============================================================================

async function runBenchmarks() {
  console.log('Node.js DI Library Benchmark');
  console.log('============================\n');
  console.log(`Node.js: ${process.version}`);
  console.log(`Platform: ${process.platform} ${process.arch}\n`);

  // ==========================================================================
  // Benchmark 1: Singleton Resolution
  // ==========================================================================

  console.log('1. Singleton Resolution');
  console.log('-----------------------');

  const manualContainer = new ManualContainer();
  const mapContainer = new MapContainer();
  mapContainer.register('config', new Config());
  const inversifyContainer = createInversifyContainer();
  const awilixContainer = createAwilixContainer();

  // Warm up all containers
  manualContainer.getConfig();
  mapContainer.get<Config>('config');
  inversifyContainer.get(TYPES.Config);
  awilixContainer.resolve('config');

  const singletonResults = [
    benchmark('manual_di', () => { manualContainer.getConfig(); }),
    benchmark('map_based', () => { mapContainer.get<Config>('config'); }),
    benchmark('inversify', () => { inversifyContainer.get(TYPES.Config); }),
    benchmark('awilix', () => { awilixContainer.resolve('config'); }),
  ];

  console.table(singletonResults.map(r => ({
    Library: r.name,
    'ops/sec': Math.round(r.opsPerSec).toLocaleString(),
    'avg (ns)': r.avgNs.toFixed(2),
  })));

  // ==========================================================================
  // Benchmark 2: Deep Dependency Chain
  // ==========================================================================

  console.log('\n2. Deep Dependency Chain (4 levels)');
  console.log('------------------------------------');

  const config = new Config();
  const db = new Database(config);
  const repo = new UserRepository(db);
  const svc = new UserService(repo);
  mapContainer.register('userService', svc);

  // Warm up
  manualContainer.getUserService();
  mapContainer.get<UserService>('userService');
  inversifyContainer.get(TYPES.UserService);
  awilixContainer.resolve('userService');

  const deepResults = [
    benchmark('manual_di', () => { manualContainer.getUserService(); }),
    benchmark('map_based', () => { mapContainer.get<UserService>('userService'); }),
    benchmark('inversify', () => { inversifyContainer.get(TYPES.UserService); }),
    benchmark('awilix', () => { awilixContainer.resolve('userService'); }),
  ];

  console.table(deepResults.map(r => ({
    Library: r.name,
    'ops/sec': Math.round(r.opsPerSec).toLocaleString(),
    'avg (ns)': r.avgNs.toFixed(2),
  })));

  // ==========================================================================
  // Benchmark 3: Container Creation
  // ==========================================================================

  console.log('\n3. Container Creation');
  console.log('---------------------');

  const creationResults = [
    benchmark('manual_di', () => { new ManualContainer(); }, 10000),
    benchmark('map_based', () => {
      const c = new MapContainer();
      c.register('config', new Config());
    }, 10000),
    benchmark('inversify', () => { createInversifyContainer(); }, 1000),
    benchmark('awilix', () => { createAwilixContainer(); }, 1000),
  ];

  console.table(creationResults.map(r => ({
    Library: r.name,
    'ops/sec': Math.round(r.opsPerSec).toLocaleString(),
    'avg (ns)': r.avgNs.toFixed(2),
  })));

  // ==========================================================================
  // Benchmark 4: Mixed Workload (100 operations)
  // ==========================================================================

  console.log('\n4. Mixed Workload (100 operations per iteration)');
  console.log('------------------------------------------------');

  const mixedResults = [
    benchmark('manual_di', () => {
      for (let i = 0; i < 100; i++) {
        const op = i % 20;
        if (op < 16) {
          manualContainer.getConfig();
        } else if (op < 19) {
          manualContainer.getDatabase();
        } else {
          new ManualContainer();
        }
      }
    }, 10000),
    benchmark('map_based', () => {
      for (let i = 0; i < 100; i++) {
        const op = i % 20;
        if (op < 16) {
          mapContainer.get('config');
        } else if (op < 19) {
          mapContainer.get('database');
        } else {
          const scope = new MapContainer();
          scope.register('temp', new Config());
        }
      }
    }, 10000),
    benchmark('inversify', () => {
      for (let i = 0; i < 100; i++) {
        const op = i % 20;
        if (op < 16) {
          inversifyContainer.get(TYPES.Config);
        } else if (op < 19) {
          inversifyContainer.get(TYPES.Database);
        } else {
          // 5% - create a child container and resolve from it (inversify 7
          // replaced createChild() with the parent container option)
          const scope = new InversifyContainer({ parent: inversifyContainer });
          scope.get(TYPES.Config);
        }
      }
    }, 10000),
    benchmark('awilix', () => {
      for (let i = 0; i < 100; i++) {
        const op = i % 20;
        if (op < 16) {
          awilixContainer.resolve('config');
        } else if (op < 19) {
          awilixContainer.resolve('database');
        } else {
          const scope = awilixContainer.createScope();
          scope.resolve('config');
        }
      }
    }, 10000),
  ];

  console.table(mixedResults.map(r => ({
    Library: r.name,
    'ops/sec': Math.round(r.opsPerSec).toLocaleString(),
    'avg (µs)': (r.avgNs / 1000).toFixed(2),
  })));

  // ==========================================================================
  // Summary
  // ==========================================================================

  console.log('\n============================');
  console.log('Summary');
  console.log('============================\n');

  console.log('For comparison with Rust dependency-injector:');
  console.log('- Rust singleton resolution: ~17-32 ns');
  console.log('- Rust mixed workload (100 ops): ~2.2 µs');
  console.log('');
  console.log('Best Node.js times from this benchmark:');
  console.log(`- Singleton resolution: ${singletonResults[0].avgNs.toFixed(0)} ns (manual_di)`);
  console.log(`- Mixed workload: ${(mixedResults[0].avgNs / 1000).toFixed(2)} µs (manual_di)`);
}

runBenchmarks().catch(console.error);

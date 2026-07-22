#!/usr/bin/env python3
"""
Python DI Library Benchmark Comparison

Compares:
- Manual DI (baseline)
- Dict-based DI (simple runtime)
- dependency-injector (most popular Python DI)
- injector (Google's Python DI)
- punq (lightweight DI)
"""

import time
import sys
from typing import Optional
from dataclasses import dataclass

# Import DI libraries
from dependency_injector import containers, providers
from injector import Injector, inject, singleton, Module, provider
import punq

# =============================================================================
# Test Services
# =============================================================================

@dataclass
class Config:
    database_url: str = "postgres://localhost/test"
    max_connections: int = 10


@dataclass
class Database:
    config: Config


@dataclass
class UserRepository:
    db: Database
    cache_enabled: bool = True


@dataclass
class UserService:
    repo: UserRepository
    name: str = "UserService"


# =============================================================================
# Manual DI (Baseline)
# =============================================================================

class ManualContainer:
    def __init__(self):
        self._config = Config()
        self._database = Database(self._config)
        self._user_repo = UserRepository(self._database)
        self._user_service = UserService(self._user_repo)

    def get_config(self) -> Config:
        return self._config

    def get_database(self) -> Database:
        return self._database

    def get_user_service(self) -> UserService:
        return self._user_service


# =============================================================================
# Dict-based DI (Simple runtime)
# =============================================================================

class DictContainer:
    def __init__(self):
        self._services: dict = {}

    def register(self, name: str, service):
        self._services[name] = service

    def get(self, name: str):
        return self._services.get(name)


# =============================================================================
# dependency-injector setup
# =============================================================================

class DIContainer(containers.DeclarativeContainer):
    config = providers.Singleton(Config)
    database = providers.Singleton(Database, config=config)
    user_repo = providers.Singleton(UserRepository, db=database)
    user_service = providers.Singleton(UserService, repo=user_repo)


# =============================================================================
# injector (Google) setup
# =============================================================================

class InjectorModule(Module):
    @singleton
    @provider
    def provide_config(self) -> Config:
        return Config()

    @singleton
    @provider
    def provide_database(self, config: Config) -> Database:
        return Database(config)

    @singleton
    @provider
    def provide_user_repo(self, db: Database) -> UserRepository:
        return UserRepository(db)

    @singleton
    @provider
    def provide_user_service(self, repo: UserRepository) -> UserService:
        return UserService(repo)


# =============================================================================
# punq setup
# =============================================================================

def create_punq_container() -> punq.Container:
    container = punq.Container()
    container.register(Config, scope=punq.Scope.singleton)
    container.register(Database, scope=punq.Scope.singleton)
    container.register(UserRepository, scope=punq.Scope.singleton)
    container.register(UserService, scope=punq.Scope.singleton)
    return container


# =============================================================================
# Benchmark utilities
# =============================================================================

def benchmark(name: str, fn, iterations: int = 100000) -> dict:
    """Run a benchmark and return results."""
    # Warm up
    for _ in range(1000):
        fn()

    # Benchmark
    start = time.perf_counter_ns()
    for _ in range(iterations):
        fn()
    end = time.perf_counter_ns()

    total_ns = end - start
    avg_ns = total_ns / iterations
    ops_per_sec = 1e9 / avg_ns

    return {
        "name": name,
        "ops_per_sec": ops_per_sec,
        "avg_ns": avg_ns,
    }


def print_table(results: list[dict], time_unit: str = "ns"):
    """Print results as a table."""
    print(f"{'Library':<25} {'ops/sec':>15} {'avg ({})':>15}".format(time_unit))
    print("-" * 57)
    for r in results:
        if time_unit == "ns":
            time_val = f"{r['avg_ns']:.2f}"
        elif time_unit == "µs":
            time_val = f"{r['avg_ns'] / 1000:.2f}"
        else:
            time_val = f"{r['avg_ns']:.2f}"
        print(f"{r['name']:<25} {r['ops_per_sec']:>15,.0f} {time_val:>15}")
    print()


# =============================================================================
# Main benchmark
# =============================================================================

def main():
    print("Python DI Library Benchmark")
    print("===========================\n")
    print(f"Python: {sys.version}")
    print()

    # =========================================================================
    # Benchmark 1: Singleton Resolution
    # =========================================================================

    print("1. Singleton Resolution")
    print("-----------------------")

    # Setup containers
    manual = ManualContainer()

    dict_container = DictContainer()
    dict_container.register("config", Config())

    di_container = DIContainer()
    di_container.config()  # Warm up singleton

    google_injector = Injector([InjectorModule()])
    google_injector.get(Config)  # Warm up

    punq_container = create_punq_container()
    punq_container.resolve(Config)  # Warm up

    singleton_results = [
        benchmark("manual_di", lambda: manual.get_config()),
        benchmark("dict_based", lambda: dict_container.get("config")),
        benchmark("dependency-injector", lambda: di_container.config()),
        benchmark("injector (Google)", lambda: google_injector.get(Config)),
        benchmark("punq", lambda: punq_container.resolve(Config)),
    ]

    print_table(singleton_results, "ns")

    # =========================================================================
    # Benchmark 2: Deep Dependency Chain
    # =========================================================================

    print("2. Deep Dependency Chain (4 levels)")
    print("------------------------------------")

    # Setup dict container with full chain
    config = Config()
    db = Database(config)
    repo = UserRepository(db)
    svc = UserService(repo)
    dict_container.register("user_service", svc)

    # Warm up
    manual.get_user_service()
    di_container.user_service()
    google_injector.get(UserService)
    punq_container.resolve(UserService)

    deep_results = [
        benchmark("manual_di", lambda: manual.get_user_service()),
        benchmark("dict_based", lambda: dict_container.get("user_service")),
        benchmark("dependency-injector", lambda: di_container.user_service()),
        benchmark("injector (Google)", lambda: google_injector.get(UserService)),
        benchmark("punq", lambda: punq_container.resolve(UserService)),
    ]

    print_table(deep_results, "ns")

    # =========================================================================
    # Benchmark 3: Container Creation
    # =========================================================================

    print("3. Container Creation")
    print("---------------------")

    creation_results = [
        benchmark("manual_di", lambda: ManualContainer(), iterations=10000),
        benchmark("dict_based", lambda: DictContainer(), iterations=10000),
        benchmark("dependency-injector", lambda: DIContainer(), iterations=1000),
        benchmark("injector (Google)", lambda: Injector([InjectorModule()]), iterations=1000),
        benchmark("punq", lambda: create_punq_container(), iterations=1000),
    ]

    print_table(creation_results, "ns")

    # =========================================================================
    # Benchmark 4: Mixed Workload (100 operations)
    # =========================================================================

    print("4. Mixed Workload (100 operations per iteration)")
    print("------------------------------------------------")

    def mixed_manual():
        for i in range(100):
            op = i % 20
            if op < 16:
                manual.get_config()
            elif op < 19:
                manual.get_database()
            else:
                ManualContainer()

    # NOTE: the library benchmarks' 5% branch is intentionally per-library
    # (create-only vs create+register) because scope APIs differ; see each
    # mixed_* function's inline comment for the deliberate deviation.
    def mixed_dict():
        for i in range(100):
            op = i % 20
            if op < 16:
                dict_container.get("config")
            elif op < 19:
                dict_container.get("database")
            else:
                d = DictContainer()
                d.register("temp", Config())

    def mixed_di():
        for i in range(100):
            op = i % 20
            if op < 16:
                di_container.config()
            elif op < 19:
                di_container.database()
            else:
                # 5% - instantiate a fresh declarative container (scope
                # analogue, mirrors peers' new-container work)
                DIContainer()

    def mixed_google():
        for i in range(100):
            op = i % 20
            if op < 16:
                google_injector.get(Config)
            elif op < 19:
                google_injector.get(Database)
            else:
                # 5% - create a child injector (injector's real scope
                # mechanism)
                google_injector.create_child_injector()

    def mixed_punq():
        for i in range(100):
            op = i % 20
            if op < 16:
                punq_container.resolve(Config)
            elif op < 19:
                punq_container.resolve(Database)
            else:
                # 5% - punq (0.7.x) has no child-scope API, so construct a
                # fresh container and register a service into it, mirroring
                # the dict peer's work
                scope = punq.Container()
                scope.register(Config, instance=Config())

    mixed_results = [
        benchmark("manual_di", mixed_manual, iterations=10000),
        benchmark("dict_based", mixed_dict, iterations=10000),
        benchmark("dependency-injector", mixed_di, iterations=10000),
        benchmark("injector (Google)", mixed_google, iterations=10000),
        benchmark("punq", mixed_punq, iterations=10000),
    ]

    print_table(mixed_results, "µs")

    # =========================================================================
    # Summary
    # =========================================================================

    print("============================")
    print("Summary")
    print("============================\n")

    print("For comparison with Rust dependency-injector:")
    print("- Rust singleton resolution: ~17-32 ns")
    print("- Rust mixed workload (100 ops): ~2.2 µs")
    print()
    print("Best Python times from this benchmark:")
    print(f"- Singleton resolution: {singleton_results[0]['avg_ns']:.0f} ns (manual_di)")
    print(f"- Mixed workload: {mixed_results[0]['avg_ns'] / 1000:.2f} µs (manual_di)")


if __name__ == "__main__":
    main()




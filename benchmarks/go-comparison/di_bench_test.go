// Package main provides benchmarks comparing Go DI libraries
//
// Libraries compared:
// - Manual DI (baseline)
// - go.uber.org/dig (Uber's reflection-based DI)
// - github.com/samber/do (Generic DI with Go 1.18+ generics)
// - github.com/goioc/di (IoC container)
package main

import (
	"strconv"
	"sync"
	"testing"

	"github.com/goioc/di"
	"github.com/samber/do/v2"
	"go.uber.org/dig"
)

// =============================================================================
// Test Services
// =============================================================================

// Config is a simple value service
type Config struct {
	DatabaseURL    string
	MaxConnections int
}

func NewConfig() *Config {
	return &Config{
		DatabaseURL:    "postgres://localhost/test",
		MaxConnections: 10,
	}
}

// Database is a service with a dependency
type Database struct {
	Config *Config
}

func NewDatabase(config *Config) *Database {
	return &Database{Config: config}
}

// UserRepository is a service with multiple dependencies
type UserRepository struct {
	DB           *Database
	CacheEnabled bool
}

func NewUserRepository(db *Database) *UserRepository {
	return &UserRepository{
		DB:           db,
		CacheEnabled: true,
	}
}

// UserService is a top-level service with deep dependency chain
type UserService struct {
	Repo *UserRepository
	Name string
}

func NewUserService(repo *UserRepository) *UserService {
	return &UserService{
		Repo: repo,
		Name: "UserService",
	}
}

// =============================================================================
// Manual DI (Baseline)
// =============================================================================

type ManualContainer struct {
	config      *Config
	database    *Database
	userRepo    *UserRepository
	userService *UserService
}

func NewManualContainer() *ManualContainer {
	config := NewConfig()
	database := NewDatabase(config)
	userRepo := NewUserRepository(database)
	userService := NewUserService(userRepo)

	return &ManualContainer{
		config:      config,
		database:    database,
		userRepo:    userRepo,
		userService: userService,
	}
}

func (c *ManualContainer) GetConfig() *Config {
	return c.config
}

func (c *ManualContainer) GetDatabase() *Database {
	return c.database
}

func (c *ManualContainer) GetUserService() *UserService {
	return c.userService
}

// =============================================================================
// Map-based DI (Simple runtime)
// =============================================================================

type MapContainer struct {
	mu       sync.RWMutex
	services map[string]interface{}
}

func NewMapContainer() *MapContainer {
	return &MapContainer{
		services: make(map[string]interface{}),
	}
}

func (c *MapContainer) Register(name string, service interface{}) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.services[name] = service
}

func (c *MapContainer) Get(name string) interface{} {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.services[name]
}

// =============================================================================
// sync.Map-based DI (Concurrent runtime)
// =============================================================================

type SyncMapContainer struct {
	services sync.Map
}

func NewSyncMapContainer() *SyncMapContainer {
	return &SyncMapContainer{}
}

func (c *SyncMapContainer) Register(name string, service interface{}) {
	c.services.Store(name, service)
}

func (c *SyncMapContainer) Get(name string) interface{} {
	val, _ := c.services.Load(name)
	return val
}

// =============================================================================
// Benchmarks - Singleton Resolution
// =============================================================================

func BenchmarkSingletonResolution(b *testing.B) {
	// Manual DI (baseline)
	b.Run("manual_di", func(b *testing.B) {
		container := NewManualContainer()
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = container.GetConfig()
		}
	})

	// Map + RWMutex
	b.Run("map_rwmutex", func(b *testing.B) {
		container := NewMapContainer()
		container.Register("config", NewConfig())
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = container.Get("config")
		}
	})

	// sync.Map
	b.Run("sync_map", func(b *testing.B) {
		container := NewSyncMapContainer()
		container.Register("config", NewConfig())
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = container.Get("config")
		}
	})

	// Uber dig
	b.Run("uber_dig", func(b *testing.B) {
		container := dig.New()
		_ = container.Provide(NewConfig)
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = container.Invoke(func(c *Config) {
				_ = c
			})
		}
	})

	// samber/do
	b.Run("samber_do", func(b *testing.B) {
		injector := do.New()
		do.Provide(injector, func(i do.Injector) (*Config, error) {
			return NewConfig(), nil
		})
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_, _ = do.Invoke[*Config](injector)
		}
	})

	// goioc/di
	b.Run("goioc_di", func(b *testing.B) {
		_, _ = di.RegisterBeanInstance("config", NewConfig())
		_ = di.InitializeContainer()
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = di.GetInstance("config")
		}
	})
}

// =============================================================================
// Benchmarks - Deep Dependency Chain
// =============================================================================

func BenchmarkDeepDependencyChain(b *testing.B) {
	// Manual DI (baseline)
	b.Run("manual_di", func(b *testing.B) {
		container := NewManualContainer()
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = container.GetUserService()
		}
	})

	// Map + RWMutex
	b.Run("map_rwmutex", func(b *testing.B) {
		container := NewMapContainer()
		config := NewConfig()
		db := NewDatabase(config)
		repo := NewUserRepository(db)
		svc := NewUserService(repo)
		container.Register("config", config)
		container.Register("database", db)
		container.Register("userRepo", repo)
		container.Register("userService", svc)
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = container.Get("userService")
		}
	})

	// sync.Map
	b.Run("sync_map", func(b *testing.B) {
		container := NewSyncMapContainer()
		config := NewConfig()
		db := NewDatabase(config)
		repo := NewUserRepository(db)
		svc := NewUserService(repo)
		container.Register("config", config)
		container.Register("database", db)
		container.Register("userRepo", repo)
		container.Register("userService", svc)
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = container.Get("userService")
		}
	})

	// Uber dig with full chain
	b.Run("uber_dig", func(b *testing.B) {
		container := dig.New()
		_ = container.Provide(NewConfig)
		_ = container.Provide(NewDatabase)
		_ = container.Provide(NewUserRepository)
		_ = container.Provide(NewUserService)
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = container.Invoke(func(svc *UserService) {
				_ = svc
			})
		}
	})

	// samber/do with full chain
	b.Run("samber_do", func(b *testing.B) {
		injector := do.New()
		do.Provide(injector, func(i do.Injector) (*Config, error) {
			return NewConfig(), nil
		})
		do.Provide(injector, func(i do.Injector) (*Database, error) {
			config, _ := do.Invoke[*Config](i)
			return NewDatabase(config), nil
		})
		do.Provide(injector, func(i do.Injector) (*UserRepository, error) {
			db, _ := do.Invoke[*Database](i)
			return NewUserRepository(db), nil
		})
		do.Provide(injector, func(i do.Injector) (*UserService, error) {
			repo, _ := do.Invoke[*UserRepository](i)
			return NewUserService(repo), nil
		})
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_, _ = do.Invoke[*UserService](injector)
		}
	})
}

// =============================================================================
// Benchmarks - Container Creation
// =============================================================================

func BenchmarkContainerCreation(b *testing.B) {
	b.Run("manual_di", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = NewManualContainer()
		}
	})

	b.Run("map_rwmutex", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = NewMapContainer()
		}
	})

	b.Run("sync_map", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = NewSyncMapContainer()
		}
	})

	b.Run("uber_dig", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			c := dig.New()
			_ = c.Provide(NewConfig)
		}
	})

	b.Run("samber_do", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			injector := do.New()
			do.Provide(injector, func(i do.Injector) (*Config, error) {
				return NewConfig(), nil
			})
		}
	})
}

// =============================================================================
// Benchmarks - Concurrent Access
// =============================================================================

func BenchmarkConcurrentReads(b *testing.B) {
	// Map + RWMutex
	b.Run("map_rwmutex", func(b *testing.B) {
		container := NewMapContainer()
		container.Register("config", NewConfig())
		b.RunParallel(func(pb *testing.PB) {
			for pb.Next() {
				_ = container.Get("config")
			}
		})
	})

	// sync.Map
	b.Run("sync_map", func(b *testing.B) {
		container := NewSyncMapContainer()
		container.Register("config", NewConfig())
		b.RunParallel(func(pb *testing.PB) {
			for pb.Next() {
				_ = container.Get("config")
			}
		})
	})

	// Uber dig - Note: dig is not designed for concurrent resolution
	b.Run("uber_dig", func(b *testing.B) {
		container := dig.New()
		_ = container.Provide(NewConfig)
		b.RunParallel(func(pb *testing.PB) {
			for pb.Next() {
				_ = container.Invoke(func(c *Config) {
					_ = c
				})
			}
		})
	})

	// samber/do
	b.Run("samber_do", func(b *testing.B) {
		injector := do.New()
		do.Provide(injector, func(i do.Injector) (*Config, error) {
			return NewConfig(), nil
		})
		b.RunParallel(func(pb *testing.PB) {
			for pb.Next() {
				_, _ = do.Invoke[*Config](injector)
			}
		})
	})
}

// =============================================================================
// Benchmarks - Mixed Workload
// =============================================================================

func BenchmarkMixedWorkload(b *testing.B) {
	// Map + RWMutex
	b.Run("map_rwmutex", func(b *testing.B) {
		container := NewMapContainer()
		container.Register("config", NewConfig())
		container.Register("database", NewDatabase(NewConfig()))
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			for j := 0; j < 100; j++ {
				switch j % 20 {
				case 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15:
					// 80% - resolve
					_ = container.Get("config")
				case 16, 17, 18:
					// 15% - additional resolve (lookup)
					_ = container.Get("database")
				default:
					// 5% - new scope (simulate with new map)
					scope := NewMapContainer()
					scope.Register("temp", NewConfig())
				}
			}
		}
	})

	// sync.Map
	b.Run("sync_map", func(b *testing.B) {
		container := NewSyncMapContainer()
		container.Register("config", NewConfig())
		container.Register("database", NewDatabase(NewConfig()))
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			for j := 0; j < 100; j++ {
				switch j % 20 {
				case 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15:
					_ = container.Get("config")
				case 16, 17, 18:
					_ = container.Get("database")
				default:
					scope := NewSyncMapContainer()
					scope.Register("temp", NewConfig())
				}
			}
		}
	})

	// samber/do
	b.Run("samber_do", func(b *testing.B) {
		injector := do.New()
		do.Provide(injector, func(i do.Injector) (*Config, error) {
			return NewConfig(), nil
		})
		do.Provide(injector, func(i do.Injector) (*Database, error) {
			return NewDatabase(NewConfig()), nil
		})
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			for j := 0; j < 100; j++ {
				switch j % 20 {
				case 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15:
					_, _ = do.Invoke[*Config](injector)
				case 16, 17, 18:
					_, _ = do.Invoke[*Database](injector)
				default:
					// 5% - create a child scope and register a service in it
					// (mirrors peers' new-container + register work; samber/do
					// requires unique scope names, so derive one from the loop
					// counters)
					scope := injector.Scope(strconv.Itoa(i*100 + j))
					do.ProvideValue(scope, NewConfig())
				}
			}
		}
	})
}


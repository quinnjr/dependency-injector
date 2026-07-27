// Example Go application using the dependency-injector FFI bindings.
//
// To run this example:
//
//  1. Build the Rust library:
//     cd /path/to/dependency-injector
//     cargo rustc --release --features ffi --crate-type cdylib
//
//  2. Set the library path:
//     export LD_LIBRARY_PATH=/path/to/dependency-injector/target/release:$LD_LIBRARY_PATH
//
//  3. Run the example:
//     cd ffi/go/example
//     go run main.go
package main

import (
	"errors"
	"fmt"
	"log"

	"github.com/pegasusheavy/dependency-injector/ffi/go/di"
)

// Config represents application configuration.
type Config struct {
	Debug    bool   `json:"debug"`
	Port     int    `json:"port"`
	DBHost   string `json:"db_host"`
	LogLevel string `json:"log_level"`
}

// User represents a user entity.
type User struct {
	ID    int    `json:"id"`
	Name  string `json:"name"`
	Email string `json:"email"`
}

// DatabaseService represents a database connection.
type DatabaseService struct {
	Host     string `json:"host"`
	Database string `json:"database"`
	PoolSize int    `json:"pool_size"`
}

func main() {
	fmt.Println("╔════════════════════════════════════════════════════════════╗")
	fmt.Println("║          dependency-injector Go Example                     ║")
	fmt.Println("╚════════════════════════════════════════════════════════════╝")
	fmt.Printf("\nLibrary version: %s\n\n", di.Version())

	// Create the root container
	container := di.NewContainer()
	if container == nil {
		log.Fatal("Failed to create container")
	}
	defer container.Free()
	fmt.Println("✓ Created root container")

	// === Register Application Services ===
	fmt.Println("\n--- Registering Services ---")

	// Register application configuration
	config := Config{
		Debug:    true,
		Port:     8080,
		DBHost:   "localhost:5432",
		LogLevel: "debug",
	}
	if err := container.RegisterValue("Config", config); err != nil {
		log.Fatalf("Failed to register config: %v", err)
	}
	fmt.Println("✓ Registered Config")

	// Register database service
	dbService := DatabaseService{
		Host:     "localhost:5432",
		Database: "myapp",
		PoolSize: 10,
	}
	if err := container.RegisterValue("DatabaseService", dbService); err != nil {
		log.Fatalf("Failed to register database: %v", err)
	}
	fmt.Println("✓ Registered DatabaseService")

	// === Check Container State ===
	fmt.Println("\n--- Container State ---")
	serviceCount, err := container.ServiceCount()
	if err != nil {
		log.Fatalf("Failed to read service count: %v", err)
	}
	fmt.Printf("Service count: %d\n", serviceCount)
	for _, name := range []string{"Config", "DatabaseService", "NonExistent"} {
		present, err := container.Contains(name)
		if err != nil {
			log.Fatalf("Failed to check for %q: %v", name, err)
		}
		fmt.Printf("Contains %q: %v\n", name, present)
	}

	// === Resolve Services ===
	fmt.Println("\n--- Resolving Services ---")

	var resolvedConfig Config
	if err := container.ResolveJSON("Config", &resolvedConfig); err != nil {
		log.Fatalf("Failed to resolve config: %v", err)
	}
	fmt.Printf("✓ Resolved Config: debug=%v, port=%d, log_level=%s\n",
		resolvedConfig.Debug, resolvedConfig.Port, resolvedConfig.LogLevel)

	var resolvedDB DatabaseService
	if err := container.ResolveInto("DatabaseService", &resolvedDB); err != nil {
		log.Fatalf("Failed to resolve database: %v", err)
	}
	fmt.Printf("✓ Resolved DatabaseService: %s/%s (pool: %d)\n",
		resolvedDB.Host, resolvedDB.Database, resolvedDB.PoolSize)

	// === TryResolve Demo ===
	fmt.Println("\n--- Optional Resolution ---")

	// TryResolve returns nil for missing services (no error)
	if data := container.TryResolve("NonExistent"); data == nil {
		fmt.Println("✓ TryResolve returned nil for missing service")
	}

	if data := container.TryResolve("Config"); data != nil {
		fmt.Println("✓ TryResolve returned data for existing service")
	}

	// === Scoped Containers ===
	fmt.Println("\n--- Scoped Containers ---")

	// Create a request scope
	requestScope, err := container.Scope()
	if err != nil {
		log.Fatalf("Failed to create scope: %v", err)
	}
	defer requestScope.Free()
	fmt.Println("✓ Created request scope")

	// Register request-specific data
	user := User{
		ID:    42,
		Name:  "Alice",
		Email: "alice@example.com",
	}
	if err := requestScope.RegisterValue("CurrentUser", user); err != nil {
		log.Fatalf("Failed to register user: %v", err)
	}
	fmt.Println("✓ Registered CurrentUser in request scope")

	// Request scope can access parent services
	var dbFromScope DatabaseService
	if err := requestScope.ResolveJSON("DatabaseService", &dbFromScope); err != nil {
		log.Fatalf("Failed to resolve from scope: %v", err)
	}
	fmt.Printf("✓ Request scope can access DatabaseService: %s\n", dbFromScope.Database)

	// Resolve the user from the request scope
	var resolvedUser User
	if err := requestScope.ResolveJSON("CurrentUser", &resolvedUser); err != nil {
		log.Fatalf("Failed to resolve user: %v", err)
	}
	fmt.Printf("✓ Resolved CurrentUser: %s <%s>\n", resolvedUser.Name, resolvedUser.Email)

	// Parent cannot access scoped services
	parentHasUser, err := container.Contains("CurrentUser")
	if err != nil {
		log.Fatalf("Failed to check parent for CurrentUser: %v", err)
	}
	if parentHasUser {
		log.Fatal("Parent should not contain CurrentUser!")
	}
	fmt.Println("✓ Parent container correctly doesn't see CurrentUser")

	// Nested scopes
	nestedScope, err := requestScope.Scope()
	if err != nil {
		log.Fatalf("Failed to create nested scope: %v", err)
	}
	defer nestedScope.Free()
	nestedScope.RegisterValue("NestedData", map[string]int{"level": 2})
	fmt.Println("✓ Created nested scope with data")

	// Nested scope can access all ancestors
	var configFromNested Config
	if err := nestedScope.ResolveJSON("Config", &configFromNested); err != nil {
		log.Fatalf("Failed to resolve from nested: %v", err)
	}
	fmt.Printf("✓ Nested scope resolved root Config: log_level=%s\n", configFromNested.LogLevel)

	// === Error Handling Demo ===
	fmt.Println("\n--- Error Handling ---")

	_, err = container.Resolve("NonExistentService")
	if err != nil {
		fmt.Printf("✓ Got expected error: %v\n", err)

		// Check error type
		if errors.Is(err, di.ErrNotFound) {
			fmt.Println("✓ Error is ErrNotFound")
		}
	}

	// Try to register duplicate
	err = container.RegisterValue("Config", Config{})
	if err != nil {
		fmt.Printf("✓ Got expected error: %v\n", err)

		if errors.Is(err, di.ErrAlreadyRegistered) {
			fmt.Println("✓ Error is ErrAlreadyRegistered")
		}
	}

	fmt.Println("\n✅ All demos completed successfully!")
}

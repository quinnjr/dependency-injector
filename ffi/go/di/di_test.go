package di

import (
	"errors"
	"testing"
)

func TestNewContainer(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	if mustCount(t, container) != 0 {
		t.Errorf("Expected 0 services, got %d", mustCount(t, container))
	}
}

func TestRegisterAndResolve(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	// Register a service
	err := container.Register("TestService", []byte("hello world"))
	if err != nil {
		t.Fatalf("Failed to register: %v", err)
	}

	// Resolve the service
	data, err := container.Resolve("TestService")
	if err != nil {
		t.Fatalf("Failed to resolve: %v", err)
	}

	if string(data) != "hello world" {
		t.Errorf("Expected 'hello world', got '%s'", string(data))
	}
}

func TestRegisterJSON(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	// Register with JSON
	err := container.RegisterJSON("Config", `{"debug": true, "port": 8080}`)
	if err != nil {
		t.Fatalf("Failed to register JSON: %v", err)
	}

	// Resolve and parse
	var config struct {
		Debug bool `json:"debug"`
		Port  int  `json:"port"`
	}
	err = container.ResolveJSON("Config", &config)
	if err != nil {
		t.Fatalf("Failed to resolve JSON: %v", err)
	}

	if !config.Debug {
		t.Error("Expected debug to be true")
	}
	if config.Port != 8080 {
		t.Errorf("Expected port 8080, got %d", config.Port)
	}
}

func TestRegisterValue(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	type User struct {
		ID   int    `json:"id"`
		Name string `json:"name"`
	}

	// Register a struct value
	err := container.RegisterValue("User", User{ID: 1, Name: "Alice"})
	if err != nil {
		t.Fatalf("Failed to register value: %v", err)
	}

	// Resolve it back
	var user User
	err = container.ResolveJSON("User", &user)
	if err != nil {
		t.Fatalf("Failed to resolve: %v", err)
	}

	if user.ID != 1 || user.Name != "Alice" {
		t.Errorf("Expected {1, Alice}, got {%d, %s}", user.ID, user.Name)
	}
}

func TestResolveInto(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	type Config struct {
		Debug bool   `json:"debug"`
		Host  string `json:"host"`
	}

	err := container.RegisterValue("Config", Config{Debug: true, Host: "localhost"})
	if err != nil {
		t.Fatalf("Failed to register: %v", err)
	}

	var config Config
	err = container.ResolveInto("Config", &config)
	if err != nil {
		t.Fatalf("Failed to resolve into: %v", err)
	}

	if !config.Debug || config.Host != "localhost" {
		t.Errorf("Unexpected config: %+v", config)
	}
}

func TestTryResolve(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	// TryResolve on non-existent should return nil
	data := container.TryResolve("NonExistent")
	if data != nil {
		t.Error("Expected nil for non-existent service")
	}

	// Register and try again
	container.Register("Exists", []byte("data"))
	data = container.TryResolve("Exists")
	if data == nil {
		t.Error("Expected data for existing service")
	}
	if string(data) != "data" {
		t.Errorf("Expected 'data', got '%s'", string(data))
	}
}

// mustContain calls Contains and fails the test if it reports an error, so
// that tests which only care about the boolean stay readable.
func mustContain(t *testing.T, c *Container, typeName string) bool {
	t.Helper()
	found, err := c.Contains(typeName)
	if err != nil {
		t.Fatalf("Contains(%q) failed: %v", typeName, err)
	}
	return found
}

func mustCount(t *testing.T, c *Container) int64 {
	t.Helper()
	count, err := c.ServiceCount()
	if err != nil {
		t.Fatalf("ServiceCount() failed: %v", err)
	}
	return count
}

func TestContains(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	if mustContain(t, container, "NonExistent") {
		t.Error("Expected Contains to return false for non-existent service")
	}

	container.Register("Exists", []byte("data"))

	if !mustContain(t, container, "Exists") {
		t.Error("Expected Contains to return true for registered service")
	}
}

func TestContainsErrorIsNotCollapsedToFalse(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	container.Free()

	found, err := container.Contains("Anything")
	if err == nil {
		t.Fatal("Expected an error from Contains on a freed container")
	}
	if found {
		t.Error("Expected found to be false when Contains reports an error")
	}
}

func TestScope(t *testing.T) {
	parent := NewContainer()
	if parent == nil {
		t.Fatal("Failed to create parent container")
	}
	defer parent.Free()

	// Register in parent
	parent.Register("ParentService", []byte("parent"))

	// Create child scope
	child, err := parent.Scope()
	if err != nil {
		t.Fatalf("Failed to create scope: %v", err)
	}
	defer child.Free()

	// Child should inherit parent's services
	if !mustContain(t, child, "ParentService") {
		t.Error("Child should contain parent's service")
	}

	// Register in child
	child.Register("ChildService", []byte("child"))

	// Parent should NOT have child's service
	if mustContain(t, parent, "ChildService") {
		t.Error("Parent should not contain child's service")
	}
}

func TestNestedScopes(t *testing.T) {
	root := NewContainer()
	if root == nil {
		t.Fatal("Failed to create root container")
	}
	defer root.Free()

	root.Register("Root", []byte("root"))

	level1, err := root.Scope()
	if err != nil {
		t.Fatalf("Failed to create level1 scope: %v", err)
	}
	defer level1.Free()
	level1.Register("Level1", []byte("level1"))

	level2, err := level1.Scope()
	if err != nil {
		t.Fatalf("Failed to create level2 scope: %v", err)
	}
	defer level2.Free()
	level2.Register("Level2", []byte("level2"))

	// Level2 can access all
	if !mustContain(t, level2, "Root") || !mustContain(t, level2, "Level1") || !mustContain(t, level2, "Level2") {
		t.Error("Level2 should have access to all services")
	}

	// Level1 cannot access Level2
	if mustContain(t, level1, "Level2") {
		t.Error("Level1 should not have access to Level2 services")
	}

	// Root cannot access Level1 or Level2
	if mustContain(t, root, "Level1") || mustContain(t, root, "Level2") {
		t.Error("Root should not have access to child services")
	}
}

func TestNotFound(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	_, err := container.Resolve("NonExistent")
	if err == nil {
		t.Fatal("Expected error for non-existent service")
	}

	diErr, ok := err.(*DIError)
	if !ok {
		t.Fatalf("Expected DIError, got %T", err)
	}

	if diErr.Code != NotFound {
		t.Errorf("Expected NotFound error, got %v", diErr.Code)
	}

	// Test with errors.Is
	if !errors.Is(err, ErrNotFound) {
		t.Error("Expected errors.Is to match ErrNotFound")
	}
}

func TestAlreadyRegistered(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	err := container.Register("Service", []byte("first"))
	if err != nil {
		t.Fatalf("First registration should succeed: %v", err)
	}

	err = container.Register("Service", []byte("second"))
	if err == nil {
		t.Fatal("Second registration should fail")
	}

	diErr, ok := err.(*DIError)
	if !ok {
		t.Fatalf("Expected DIError, got %T", err)
	}

	if diErr.Code != AlreadyRegistered {
		t.Errorf("Expected AlreadyRegistered error, got %v", diErr.Code)
	}

	// Test with errors.Is
	if !errors.Is(err, ErrAlreadyRegistered) {
		t.Error("Expected errors.Is to match ErrAlreadyRegistered")
	}
}

func TestRemove(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	if err := container.Register("Doomed", []byte("data")); err != nil {
		t.Fatalf("Failed to register: %v", err)
	}
	if !mustContain(t, container, "Doomed") {
		t.Fatal("Service should be registered before removal")
	}

	removed, err := container.Remove("Doomed")
	if err != nil {
		t.Fatalf("Remove failed: %v", err)
	}
	if !removed {
		t.Error("Expected Remove to report true for a registered service")
	}

	if mustContain(t, container, "Doomed") {
		t.Error("Service should be gone after removal")
	}
	if mustCount(t, container) != 0 {
		t.Errorf("Expected 0 services after removal, got %d", mustCount(t, container))
	}

	// The name is free again, so it can be re-registered.
	if err := container.Register("Doomed", []byte("again")); err != nil {
		t.Fatalf("Failed to re-register after removal: %v", err)
	}
}

func TestRemoveNotFound(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	removed, err := container.Remove("NonExistent")
	if err != nil {
		t.Fatalf("Remove of a missing service should not error, got: %v", err)
	}
	if removed {
		t.Error("Expected Remove to report false for a non-existent service")
	}
}

func TestRemoveFreedContainer(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	container.Free()

	removed, err := container.Remove("Anything")
	if err == nil {
		t.Fatal("Expected an error from Remove on a freed container")
	}
	if removed {
		t.Error("Expected removed to be false when Remove reports an error")
	}
}

func TestClear(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	container.Register("One", []byte("1"))
	container.Register("Two", []byte("2"))
	if mustCount(t, container) != 2 {
		t.Fatalf("Expected 2 services, got %d", mustCount(t, container))
	}

	if err := container.Clear(); err != nil {
		t.Fatalf("Clear failed: %v", err)
	}

	if mustCount(t, container) != 0 {
		t.Errorf("Expected 0 services after Clear, got %d", mustCount(t, container))
	}
	if mustContain(t, container, "One") || mustContain(t, container, "Two") {
		t.Error("No service should remain after Clear")
	}

	// Clearing an already empty container is still a success.
	if err := container.Clear(); err != nil {
		t.Errorf("Clear on an empty container should succeed, got: %v", err)
	}
}

func TestIsLocked(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	locked, err := container.IsLocked()
	if err != nil {
		t.Fatalf("IsLocked failed: %v", err)
	}
	if locked {
		t.Error("A new container should not be locked")
	}

	if err := container.Lock(); err != nil {
		t.Fatalf("Lock failed: %v", err)
	}

	locked, err = container.IsLocked()
	if err != nil {
		t.Fatalf("IsLocked failed after Lock: %v", err)
	}
	if !locked {
		t.Error("Container should report locked after Lock")
	}
}

func TestIsLockedErrorIsNotCollapsedToFalse(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	container.Free()

	locked, err := container.IsLocked()
	if err == nil {
		t.Fatal("Expected an error from IsLocked on a freed container")
	}
	if locked {
		t.Error("Expected locked to be false when IsLocked reports an error")
	}
}

func TestLockBlocksRegistration(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	if err := container.Register("Before", []byte("data")); err != nil {
		t.Fatalf("Registration before Lock should succeed: %v", err)
	}

	if err := container.Lock(); err != nil {
		t.Fatalf("Lock failed: %v", err)
	}

	err := container.Register("After", []byte("data"))
	if err == nil {
		t.Fatal("Registration after Lock should fail")
	}

	diErr, ok := err.(*DIError)
	if !ok {
		t.Fatalf("Expected DIError, got %T", err)
	}
	if diErr.Code != Locked {
		t.Errorf("Expected Locked error, got %v", diErr.Code)
	}
	if !errors.Is(err, ErrLocked) {
		t.Error("Expected errors.Is to match ErrLocked")
	}

	// The other registration entry points must surface Locked too.
	if err := container.RegisterJSON("AfterJSON", `{"a":1}`); !errors.Is(err, ErrLocked) {
		t.Errorf("Expected RegisterJSON to report Locked, got: %v", err)
	}
	if err := container.RegisterValue("AfterValue", map[string]int{"a": 1}); !errors.Is(err, ErrLocked) {
		t.Errorf("Expected RegisterValue to report Locked, got: %v", err)
	}

	// The pre-existing service is untouched.
	if !mustContain(t, container, "Before") {
		t.Error("Locking should not drop already-registered services")
	}
}

func TestLockAllowsRemoveAndClear(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}
	defer container.Free()

	container.Register("Removable", []byte("data"))
	container.Register("Clearable", []byte("data"))

	if err := container.Lock(); err != nil {
		t.Fatalf("Lock failed: %v", err)
	}

	// Locking blocks registration only - removal stays permitted.
	removed, err := container.Remove("Removable")
	if err != nil {
		t.Fatalf("Remove on a locked container should succeed: %v", err)
	}
	if !removed {
		t.Error("Expected Remove to report true on a locked container")
	}

	if err := container.Clear(); err != nil {
		t.Fatalf("Clear on a locked container should succeed: %v", err)
	}
	if mustCount(t, container) != 0 {
		t.Errorf("Expected 0 services after Clear, got %d", mustCount(t, container))
	}

	// Still locked afterwards - there is no unlock.
	locked, err := container.IsLocked()
	if err != nil {
		t.Fatalf("IsLocked failed: %v", err)
	}
	if !locked {
		t.Error("Container should remain locked after Remove/Clear")
	}
}

func TestScopeStartsUnlocked(t *testing.T) {
	parent := NewContainer()
	if parent == nil {
		t.Fatal("Failed to create parent container")
	}
	defer parent.Free()

	parent.Register("Inherited", []byte("data"))
	if err := parent.Lock(); err != nil {
		t.Fatalf("Lock failed: %v", err)
	}

	child, err := parent.Scope()
	if err != nil {
		t.Fatalf("Failed to create scope: %v", err)
	}
	defer child.Free()

	locked, err := child.IsLocked()
	if err != nil {
		t.Fatalf("IsLocked failed: %v", err)
	}
	if locked {
		t.Error("A child scope should start unlocked")
	}

	if err := child.Register("ChildService", []byte("data")); err != nil {
		t.Errorf("Registration in an unlocked child scope should succeed: %v", err)
	}
}

func TestErrorCodeMessages(t *testing.T) {
	cases := []struct {
		code ErrorCode
		want string
	}{
		{OK, "ok"},
		{NotFound, "service not found"},
		{InvalidArgument, "invalid argument"},
		{AlreadyRegistered, "service already registered"},
		{InternalError, "internal error"},
		{SerializationError, "serialization error"},
		{Locked, "container is locked - registration is not allowed"},
		// An unknown code from a newer native library must degrade to a
		// generic message rather than panicking or aliasing a known code.
		{ErrorCode(7), "unknown error code: 7"},
		{ErrorCode(9999), "unknown error code: 9999"},
		{ErrorCode(-1), "unknown error code: -1"},
	}

	for _, tc := range cases {
		if got := tc.code.Error(); got != tc.want {
			t.Errorf("ErrorCode(%d).Error() = %q, want %q", int(tc.code), got, tc.want)
		}
	}
}

func TestVersion(t *testing.T) {
	version := Version()
	if version == "" {
		t.Error("Version should not be empty")
	}
	t.Logf("Library version: %s", version)
}

func TestFreeNil(t *testing.T) {
	// Free should be safe to call on nil container
	var c *Container
	c.Free() // Should not panic
}

func TestFreeTwice(t *testing.T) {
	container := NewContainer()
	if container == nil {
		t.Fatal("Failed to create container")
	}

	container.Free()
	container.Free() // Should not panic
}

func BenchmarkRegister(b *testing.B) {
	container := NewContainer()
	if container == nil {
		b.Fatal("Failed to create container")
	}
	defer container.Free()

	data := []byte(`{"id": 1, "name": "test"}`)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		typeName := "Service" + string(rune(i%1000+'A'))
		container.Register(typeName, data)
	}
}

func BenchmarkResolve(b *testing.B) {
	container := NewContainer()
	if container == nil {
		b.Fatal("Failed to create container")
	}
	defer container.Free()

	container.Register("BenchService", []byte(`{"id": 1}`))

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		container.Resolve("BenchService")
	}
}

func BenchmarkContains(b *testing.B) {
	container := NewContainer()
	if container == nil {
		b.Fatal("Failed to create container")
	}
	defer container.Free()

	container.Register("BenchService", []byte(`{"id": 1}`))

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, _ = container.Contains("BenchService")
	}
}

func BenchmarkResolveJSON(b *testing.B) {
	container := NewContainer()
	if container == nil {
		b.Fatal("Failed to create container")
	}
	defer container.Free()

	type Config struct {
		Debug bool `json:"debug"`
		Port  int  `json:"port"`
	}

	container.RegisterValue("Config", Config{Debug: true, Port: 8080})

	var config Config
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		container.ResolveJSON("Config", &config)
	}
}

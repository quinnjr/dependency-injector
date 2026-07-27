using Xunit;
using DependencyInjector;
using DependencyInjector.Native;

namespace DependencyInjector.Tests;

public class ContainerTests
{
    // Test model classes
    public record Config(bool Debug, int Port, string Host);
    public record User(int Id, string Name, string Email);
    public record DatabaseConfig(string ConnectionString, int PoolSize);

    [Fact]
    public void NewContainer_CreatesEmptyContainer()
    {
        using var container = new Container();
        Assert.Equal(0, container.ServiceCount);
    }

    [Fact]
    public void Version_ReturnsNonEmptyString()
    {
        var version = Container.Version;
        Assert.NotNull(version);
        Assert.NotEmpty(version);
        Assert.Contains(".", version); // Should be semver-like
    }

    [Fact]
    public void Register_AddsService()
    {
        using var container = new Container();

        container.Register("Config", new Config(true, 8080, "localhost"));

        Assert.Equal(1, container.ServiceCount);
        Assert.True(container.Contains("Config"));
    }

    [Fact]
    public void Register_WithTypeInference_AddsService()
    {
        using var container = new Container();

        container.Register(new Config(true, 8080, "localhost"));

        Assert.True(container.Contains<Config>());
    }

    [Fact]
    public void Resolve_ReturnsRegisteredService()
    {
        using var container = new Container();
        var original = new Config(true, 8080, "localhost");

        container.Register("Config", original);
        var resolved = container.Resolve<Config>("Config");

        Assert.Equal(original, resolved);
    }

    [Fact]
    public void Resolve_WithTypeInference_ReturnsService()
    {
        using var container = new Container();
        var original = new User(1, "Alice", "alice@example.com");

        container.Register(original);
        var resolved = container.Resolve<User>();

        Assert.Equal(original, resolved);
    }

    [Fact]
    public void Resolve_NotFound_ThrowsDIException()
    {
        using var container = new Container();

        var ex = Assert.Throws<DIException>(() => container.Resolve<Config>("NonExistent"));
        Assert.Equal(DiErrorCode.NotFound, ex.ErrorCode);
    }

    [Fact]
    public void TryResolve_ReturnsServiceIfFound()
    {
        using var container = new Container();
        var original = new Config(true, 8080, "localhost");

        container.Register("Config", original);
        var resolved = container.TryResolve<Config>("Config");

        Assert.NotNull(resolved);
        Assert.Equal(original, resolved);
    }

    [Fact]
    public void TryResolve_ReturnsNullIfNotFound()
    {
        using var container = new Container();

        var resolved = container.TryResolve<Config>("NonExistent");

        Assert.Null(resolved);
    }

    [Fact]
    public void Contains_ReturnsTrueForRegisteredService()
    {
        using var container = new Container();

        container.Register("Config", new Config(true, 8080, "localhost"));

        Assert.True(container.Contains("Config"));
    }

    [Fact]
    public void Contains_ReturnsFalseForUnregisteredService()
    {
        using var container = new Container();

        Assert.False(container.Contains("NonExistent"));
    }

    [Fact]
    public void Register_Duplicate_ThrowsDIException()
    {
        using var container = new Container();

        container.Register("Config", new Config(true, 8080, "localhost"));

        var ex = Assert.Throws<DIException>(() =>
            container.Register("Config", new Config(false, 9090, "other")));
        Assert.Equal(DiErrorCode.AlreadyRegistered, ex.ErrorCode);
    }

    [Fact]
    public void Scope_CreatesChildContainer()
    {
        using var parent = new Container();
        parent.Register("Parent", new Config(true, 8080, "localhost"));

        using var child = parent.Scope();

        Assert.NotNull(child);
    }

    [Fact]
    public void Scope_InheritsParentServices()
    {
        using var parent = new Container();
        var config = new Config(true, 8080, "localhost");
        parent.Register("Config", config);

        using var child = parent.Scope();

        Assert.True(child.Contains("Config"));
        var resolved = child.Resolve<Config>("Config");
        Assert.Equal(config, resolved);
    }

    [Fact]
    public void Scope_DoesNotLeakToParent()
    {
        using var parent = new Container();

        using var child = parent.Scope();
        child.Register("ChildOnly", new User(1, "Alice", "alice@example.com"));

        Assert.False(parent.Contains("ChildOnly"));
        Assert.True(child.Contains("ChildOnly"));
    }

    [Fact]
    public void NestedScopes_WorkCorrectly()
    {
        using var root = new Container();
        root.Register("Root", new Config(true, 8080, "localhost"));

        using var level1 = root.Scope();
        level1.Register("Level1", new User(1, "Level1User", "l1@example.com"));

        using var level2 = level1.Scope();
        level2.Register("Level2", new DatabaseConfig("conn", 10));

        // Level2 can access all
        Assert.True(level2.Contains("Root"));
        Assert.True(level2.Contains("Level1"));
        Assert.True(level2.Contains("Level2"));

        // Level1 cannot access Level2
        Assert.False(level1.Contains("Level2"));

        // Root cannot access Level1 or Level2
        Assert.False(root.Contains("Level1"));
        Assert.False(root.Contains("Level2"));
    }

    [Fact]
    public void Dispose_ReleasesResources()
    {
        var container = new Container();
        container.Register("Config", new Config(true, 8080, "localhost"));

        container.Dispose();

        Assert.Throws<ObjectDisposedException>(() => container.ServiceCount);
    }

    [Fact]
    public void Dispose_CalledMultipleTimes_IsSafe()
    {
        var container = new Container();
        container.Dispose();
        container.Dispose(); // Should not throw
    }

    [Fact]
    public void ServiceCount_ReturnsCorrectCount()
    {
        using var container = new Container();

        Assert.Equal(0, container.ServiceCount);

        container.Register("Service1", new Config(true, 8080, "localhost"));
        Assert.Equal(1, container.ServiceCount);

        container.Register("Service2", new User(1, "Alice", "alice@example.com"));
        Assert.Equal(2, container.ServiceCount);

        container.Register("Service3", new DatabaseConfig("conn", 10));
        Assert.Equal(3, container.ServiceCount);
    }

    [Fact]
    public void Register_VariousTypes()
    {
        using var container = new Container();

        // Record types
        container.Register("Config", new Config(true, 8080, "localhost"));

        // Anonymous types don't work well with JSON deserialization,
        // but arrays and dictionaries do
        container.Register("IntArray", new int[] { 1, 2, 3 });
        container.Register("StringList", new List<string> { "a", "b", "c" });
        container.Register("Dict", new Dictionary<string, int> { { "one", 1 }, { "two", 2 } });

        Assert.Equal(4, container.ServiceCount);

        // Resolve and verify
        var config = container.Resolve<Config>("Config");
        Assert.Equal(8080, config.Port);

        var intArray = container.Resolve<int[]>("IntArray");
        Assert.Equal(new int[] { 1, 2, 3 }, intArray);

        var stringList = container.Resolve<List<string>>("StringList");
        Assert.Equal(new List<string> { "a", "b", "c" }, stringList);

        var dict = container.Resolve<Dictionary<string, int>>("Dict");
        Assert.Equal(1, dict["one"]);
        Assert.Equal(2, dict["two"]);
    }

    [Fact]
    public void Remove_RegisteredService_RoundTrips()
    {
        using var container = new Container();

        container.Register("Config", new Config(true, 8080, "localhost"));
        Assert.True(container.Contains("Config"));
        Assert.Equal(1, container.ServiceCount);

        Assert.True(container.Remove("Config"));

        Assert.False(container.Contains("Config"));
        Assert.Equal(0, container.ServiceCount);

        // The name is free again after removal.
        container.Register("Config", new Config(false, 9090, "other"));
        Assert.Equal(9090, container.Resolve<Config>("Config").Port);
    }

    [Fact]
    public void Remove_UnregisteredService_ReturnsFalse()
    {
        using var container = new Container();

        Assert.False(container.Remove("NonExistent"));
    }

    [Fact]
    public void Remove_WithTypeInference_RemovesService()
    {
        using var container = new Container();

        container.Register(new User(1, "Alice", "alice@example.com"));
        Assert.True(container.Contains<User>());

        Assert.True(container.Remove<User>());
        Assert.False(container.Contains<User>());
    }

    [Fact]
    public void Clear_RemovesAllServices()
    {
        using var container = new Container();

        container.Register("Config", new Config(true, 8080, "localhost"));
        container.Register("User", new User(1, "Alice", "alice@example.com"));
        container.Register("Db", new DatabaseConfig("conn", 10));
        Assert.Equal(3, container.ServiceCount);

        container.Clear();

        Assert.Equal(0, container.ServiceCount);
        Assert.False(container.Contains("Config"));
        Assert.False(container.Contains("User"));
        Assert.False(container.Contains("Db"));
    }

    [Fact]
    public void Clear_EmptyContainer_IsSafe()
    {
        using var container = new Container();

        container.Clear();

        Assert.Equal(0, container.ServiceCount);
    }

    [Fact]
    public void IsLocked_IsFalseUntilLocked()
    {
        using var container = new Container();

        Assert.False(container.IsLocked);

        container.Lock();

        Assert.True(container.IsLocked);
    }

    [Fact]
    public void Lock_ThenRegister_ThrowsWithLockedCode()
    {
        using var container = new Container();
        container.Lock();

        var ex = Assert.Throws<DIException>(() =>
            container.Register("Config", new Config(true, 8080, "localhost")));

        Assert.Equal(DiErrorCode.Locked, ex.ErrorCode);
        Assert.NotEmpty(ex.Message);
        Assert.Equal(0, container.ServiceCount);
    }

    [Fact]
    public void Lock_ThenRegisterWithTypeInference_ThrowsWithLockedCode()
    {
        using var container = new Container();
        container.Lock();

        var ex = Assert.Throws<DIException>(() =>
            container.Register(new User(1, "Alice", "alice@example.com")));

        Assert.Equal(DiErrorCode.Locked, ex.ErrorCode);
    }

    [Fact]
    public void Lock_StillPermitsRemove()
    {
        using var container = new Container();
        container.Register("Config", new Config(true, 8080, "localhost"));

        container.Lock();

        Assert.True(container.Remove("Config"));
        Assert.False(container.Contains("Config"));
        Assert.True(container.IsLocked);
    }

    [Fact]
    public void Lock_StillPermitsClear()
    {
        using var container = new Container();
        container.Register("Config", new Config(true, 8080, "localhost"));
        container.Register("User", new User(1, "Alice", "alice@example.com"));

        container.Lock();
        container.Clear();

        Assert.Equal(0, container.ServiceCount);
        Assert.True(container.IsLocked);
    }

    [Fact]
    public void Lock_IsIdempotent()
    {
        using var container = new Container();

        container.Lock();
        container.Lock();

        Assert.True(container.IsLocked);
    }

    [Fact]
    public void Lock_DoesNotLockExistingResolution()
    {
        using var container = new Container();
        var config = new Config(true, 8080, "localhost");
        container.Register("Config", config);

        container.Lock();

        Assert.Equal(config, container.Resolve<Config>("Config"));
    }

    [Fact]
    public void Scope_OfLockedContainer_StartsUnlocked()
    {
        using var parent = new Container();
        parent.Lock();

        using var child = parent.Scope();

        Assert.False(child.IsLocked);
        child.Register("ChildOnly", new User(1, "Alice", "alice@example.com"));
        Assert.True(child.Contains("ChildOnly"));
    }

    [Fact]
    public void NewMembers_ThrowAfterDispose()
    {
        var container = new Container();
        container.Dispose();

        Assert.Throws<ObjectDisposedException>(() => { container.Remove("Config"); });
        Assert.Throws<ObjectDisposedException>(() => container.Clear());
        Assert.Throws<ObjectDisposedException>(() => container.Lock());
        Assert.Throws<ObjectDisposedException>(() => container.IsLocked);
        Assert.Throws<ObjectDisposedException>(() => { container.Contains("Config"); });
    }
}




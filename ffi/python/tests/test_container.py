"""
Unit tests for the dependency-injector Python bindings.

Run with: pytest tests/
"""

from __future__ import annotations

import pytest
import sys
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

from dependency_injector import Container, DIError, ErrorCode


class TestContainer:
    """Tests for the Container class."""

    def test_create_container(self):
        """Should create a new container."""
        container = Container()
        assert container.service_count == 0
        container.free()

    def test_version(self):
        """Should return version string."""
        version = Container.version()
        assert version
        assert "." in version  # Should be semver-like

    def test_register_service(self):
        """Should register a service."""
        container = Container()
        container.register("Config", {"debug": True})
        assert container.contains("Config")
        assert container.service_count == 1
        container.free()

    def test_register_multiple_services(self):
        """Should register multiple services."""
        container = Container()
        container.register("Service1", {"id": 1})
        container.register("Service2", {"id": 2})
        container.register("Service3", {"id": 3})
        assert container.service_count == 3
        container.free()

    def test_register_duplicate_raises(self):
        """Should raise when registering duplicate."""
        container = Container()
        container.register("Config", {"first": True})
        with pytest.raises(DIError) as exc_info:
            container.register("Config", {"second": True})
        assert exc_info.value.code == ErrorCode.ALREADY_REGISTERED
        container.free()

    def test_contains_false_for_missing(self):
        """Should return False for non-existent service."""
        container = Container()
        assert not container.contains("NonExistent")
        container.free()

    def test_contains_true_for_registered(self):
        """Should return True for registered service."""
        container = Container()
        container.register("Exists", {})
        assert container.contains("Exists")
        container.free()

    def test_register_various_types(self):
        """Should register various JSON-serializable types."""
        container = Container()
        container.register("Dict", {"key": "value"})
        container.register("List", [1, 2, 3])
        container.register("String", "hello")
        container.register("Number", 42)
        container.register("Float", 3.14)
        container.register("Bool", True)
        container.register("Null", None)
        assert container.service_count == 7
        container.free()

    def test_register_bytes(self):
        """Should register raw bytes."""
        container = Container()
        assert container.service_count == 0
        container.register_bytes("Blob", b"\x00\x01\xffraw-bytes")
        assert container.contains("Blob")
        assert container.service_count == 1
        container.free()

    def test_register_bytes_duplicate_raises(self):
        """Should raise when registering duplicate raw bytes."""
        container = Container()
        container.register_bytes("Blob", b"first")
        with pytest.raises(DIError) as exc_info:
            container.register_bytes("Blob", b"second")
        assert exc_info.value.code == ErrorCode.ALREADY_REGISTERED
        container.free()

    def test_resolve_round_trip_special_characters(self):
        """Non-ASCII and escaped-quote payloads must survive the native
        decode path (_take_native_string) byte-for-byte."""
        container = Container()
        try:
            value = {"msg": 'h\u00e9llo "quoted" \u4e16\u754c', "n": [1, 2, 3]}
            container.register("Sp\u00e9cial", value)
            assert container.resolve("Sp\u00e9cial") == value
        finally:
            container.free()

    def test_scope_creation(self):
        """Should create a child scope."""
        container = Container()
        child = container.scope()
        assert child is not None
        child.free()
        container.free()

    def test_scope_inherits_parent(self):
        """Should inherit parent services in scope."""
        container = Container()
        container.register("Parent", {"from": "parent"})
        child = container.scope()
        assert child.contains("Parent")
        child.free()
        container.free()

    def test_scope_isolation(self):
        """Should not leak child services to parent."""
        container = Container()
        child = container.scope()
        child.register("Child", {"from": "child"})
        assert not container.contains("Child")
        assert child.contains("Child")
        child.free()
        container.free()

    def test_context_manager(self):
        """Should work as context manager."""
        with Container() as container:
            container.register("Test", {"value": 1})
            assert container.contains("Test")
        # Container is freed after with block

    def test_free_multiple_times_safe(self):
        """Should be safe to call free multiple times."""
        container = Container()
        container.free()
        container.free()  # Should not raise

    def test_use_after_free_raises(self):
        """Should raise when using freed container."""
        container = Container()
        container.free()
        with pytest.raises(DIError) as exc_info:
            container.register("Test", {})
        assert exc_info.value.code == ErrorCode.INVALID_ARGUMENT


class TestContainerResolve:
    """Tests for Container resolve functionality."""

    def test_register_and_resolve(self):
        """Should register and resolve a service."""
        container = Container()
        container.register("Config", {"debug": True, "port": 8080})
        config = container.resolve("Config")
        assert config["debug"] is True
        assert config["port"] == 8080
        container.free()

    def test_resolve_list(self):
        """Should resolve list values."""
        container = Container()
        container.register("List", [1, 2, 3])
        result = container.resolve("List")
        assert result == [1, 2, 3]
        container.free()

    def test_resolve_string(self):
        """Should resolve string values."""
        container = Container()
        container.register("Message", "Hello, World!")
        result = container.resolve("Message")
        assert result == "Hello, World!"
        container.free()

    def test_resolve_nested(self):
        """Should resolve nested objects."""
        container = Container()
        container.register("Nested", {
            "level1": {
                "level2": {
                    "value": "deep"
                }
            }
        })
        result = container.resolve("Nested")
        assert result["level1"]["level2"]["value"] == "deep"
        container.free()

    def test_resolve_not_found_raises(self):
        """Should raise for non-existent service."""
        container = Container()
        with pytest.raises(DIError) as exc_info:
            container.resolve("Missing")
        assert exc_info.value.code == ErrorCode.NOT_FOUND
        container.free()

    def test_try_resolve_returns_value(self):
        """Should return value with try_resolve."""
        container = Container()
        container.register("Config", {"debug": True})
        config = container.try_resolve("Config")
        assert config is not None
        assert config["debug"] is True
        container.free()

    def test_try_resolve_returns_none_for_missing(self):
        """Should return None for missing service with try_resolve."""
        container = Container()
        result = container.try_resolve("Missing")
        assert result is None
        container.free()

    def test_resolve_same_data_multiple_times(self):
        """Should return same data on multiple resolves."""
        container = Container()
        container.register("Config", {"id": 42})
        first = container.resolve("Config")
        second = container.resolve("Config")
        assert first["id"] == second["id"]
        container.free()


class TestScopedContainerResolve:
    """Tests for scoped container resolve functionality."""

    def test_scope_resolve_parent(self):
        """Should resolve parent services in child scope."""
        container = Container()
        container.register("Parent", {"from": "parent"})
        child = container.scope()

        # Child can resolve parent service
        parent_data = child.resolve("Parent")
        assert parent_data == {"from": "parent"}

        child.free()
        container.free()

    def test_scope_resolve_child(self):
        """Should resolve child services in child scope."""
        container = Container()
        child = container.scope()
        child.register("Child", {"from": "child"})

        # Child can resolve its own service
        child_data = child.resolve("Child")
        assert child_data == {"from": "child"}

        child.free()
        container.free()

    def test_parent_cannot_resolve_child(self):
        """Should not resolve child services in parent scope."""
        container = Container()
        child = container.scope()
        child.register("Child", {"from": "child"})

        # Parent cannot resolve child service
        with pytest.raises(DIError) as exc_info:
            container.resolve("Child")
        assert exc_info.value.code == ErrorCode.NOT_FOUND

        child.free()
        container.free()

    def test_nested_scopes(self):
        """Should support nested scopes with resolve."""
        root = Container()
        root.register("Root", {"level": 0})

        level1 = root.scope()
        level1.register("Level1", {"level": 1})

        level2 = level1.scope()
        level2.register("Level2", {"level": 2})

        # Level2 can access all
        assert level2.resolve("Root")["level"] == 0
        assert level2.resolve("Level1")["level"] == 1
        assert level2.resolve("Level2")["level"] == 2

        level2.free()
        level1.free()
        root.free()

    def test_context_manager_with_resolve(self):
        """Should work as context manager with resolve."""
        with Container() as container:
            container.register("Test", {"value": 42})
            result = container.resolve("Test")
            assert result["value"] == 42


class TestErrorHandling:
    """Tests for error handling."""

    def test_error_code_not_found(self):
        """Should have correct error code for not found."""
        container = Container()
        try:
            container.resolve("Missing")
            pytest.fail("Should have raised")
        except DIError as e:
            assert e.code == ErrorCode.NOT_FOUND
        container.free()

    def test_error_code_already_registered(self):
        """Should have correct error code for duplicate."""
        container = Container()
        container.register("Dup", {})
        try:
            container.register("Dup", {})
            pytest.fail("Should have raised")
        except DIError as e:
            assert e.code == ErrorCode.ALREADY_REGISTERED
        container.free()

    def test_error_message_formatting(self):
        """Should format error messages correctly."""
        error = DIError(ErrorCode.NOT_FOUND, "test message")
        assert "Service not found" in str(error)
        assert "test message" in str(error)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])

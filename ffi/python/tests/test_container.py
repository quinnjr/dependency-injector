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
from dependency_injector import container as container_module


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

    def test_locked_error_code_value(self):
        """LOCKED must match DI_LOCKED = 6 in the FFI header."""
        assert ErrorCode.LOCKED == 6

    def test_locked_error_message_formatting(self):
        """Should have a message mapping for LOCKED."""
        error = DIError(ErrorCode.LOCKED, "detail")
        assert "Container is locked" in str(error)
        assert "Unknown error code" not in str(error)

    def test_to_error_code_known(self):
        """Should convert known codes to their enum members."""
        assert container_module._to_error_code(6) is ErrorCode.LOCKED
        assert container_module._to_error_code(0) is ErrorCode.OK

    def test_to_error_code_unknown_falls_back(self):
        """Should not raise ValueError for a code from a newer native ABI."""
        assert container_module._to_error_code(9999) is ErrorCode.INTERNAL_ERROR
        assert container_module._to_error_code(-1) is ErrorCode.INTERNAL_ERROR

    def test_raise_native_error_preserves_unknown_code(self):
        """An unrecognized code should still be diagnosable in the message."""
        with pytest.raises(DIError) as exc_info:
            container_module._raise_native_error(9999)
        assert exc_info.value.code == ErrorCode.INTERNAL_ERROR
        assert "9999" in str(exc_info.value)


class TestContainsSentinel:
    """Tests for the -1 error sentinel from di_contains / di_is_locked.

    The FFI header states callers must not collapse -1 into False: it means
    invalid argument or a caught internal panic, not "not registered".
    """

    def test_native_contains_returns_negative_for_null_container(self):
        """The native contract itself: a null container yields -1, not 0."""
        assert container_module._lib.di_contains(None, b"Anything") == -1

    def test_native_is_locked_returns_negative_for_null_container(self):
        """The native contract itself: a null container yields -1, not 0."""
        assert container_module._lib.di_is_locked(None) == -1

    def test_contains_on_freed_container_raises(self):
        """Should raise, not return False, for a freed container."""
        container = Container()
        container.register("Config", {})
        container.free()
        with pytest.raises(DIError) as exc_info:
            container.contains("Config")
        assert exc_info.value.code == ErrorCode.INVALID_ARGUMENT

    def test_is_locked_on_freed_container_raises(self):
        """Should raise, not return False, for a freed container."""
        container = Container()
        container.free()
        with pytest.raises(DIError) as exc_info:
            container.is_locked()
        assert exc_info.value.code == ErrorCode.INVALID_ARGUMENT

    def test_contains_raises_on_negative_sentinel(self, monkeypatch):
        """A -1 return must raise rather than be reported as False.

        A live container cannot be driven to -1 without forging an invalid
        handle (undefined behaviour), so the native call is stubbed to return
        the sentinel and the binding's translation of it is asserted.
        """
        container = Container()
        try:
            monkeypatch.setattr(
                container_module._lib, "di_contains", lambda *_args: -1
            )
            with pytest.raises(DIError) as exc_info:
                container.contains("Config")
            assert exc_info.value.code in (
                ErrorCode.INVALID_ARGUMENT,
                ErrorCode.INTERNAL_ERROR,
            )
        finally:
            container.free()

    def test_is_locked_raises_on_negative_sentinel(self, monkeypatch):
        """A -1 return from di_is_locked must raise, not report False."""
        container = Container()
        try:
            monkeypatch.setattr(
                container_module._lib, "di_is_locked", lambda *_args: -1
            )
            with pytest.raises(DIError) as exc_info:
                container.is_locked()
            assert exc_info.value.code in (
                ErrorCode.INVALID_ARGUMENT,
                ErrorCode.INTERNAL_ERROR,
            )
        finally:
            container.free()


class TestRemoveAndClear:
    """Tests for remove() and clear()."""

    def test_remove_round_trip(self):
        """Should remove a registered service and report it."""
        container = Container()
        try:
            container.register("Config", {"debug": True})
            assert container.contains("Config")

            assert container.remove("Config") is True
            assert container.contains("Config") is False
            assert container.service_count == 0

            # Removing again reports False rather than raising
            assert container.remove("Config") is False
        finally:
            container.free()

    def test_remove_missing_returns_false(self):
        """Should return False for a service that was never registered."""
        container = Container()
        try:
            assert container.remove("NeverRegistered") is False
        finally:
            container.free()

    def test_remove_does_not_leak_error_to_next_call(self):
        """A NotFound removal must not poison a later operation."""
        container = Container()
        try:
            assert container.remove("Missing") is False
            # A subsequent successful resolve must not pick up the stale error
            container.register("Config", {"ok": True})
            assert container.resolve("Config") == {"ok": True}
        finally:
            container.free()

    def test_remove_only_targets_named_service(self):
        """Should leave other services intact."""
        container = Container()
        try:
            container.register("A", {"id": 1})
            container.register("B", {"id": 2})
            assert container.remove("A") is True
            assert not container.contains("A")
            assert container.contains("B")
            assert container.service_count == 1
        finally:
            container.free()

    def test_remove_on_freed_container_raises(self):
        """Should raise when removing from a freed container."""
        container = Container()
        container.free()
        with pytest.raises(DIError) as exc_info:
            container.remove("Config")
        assert exc_info.value.code == ErrorCode.INVALID_ARGUMENT

    def test_clear_removes_all_services(self):
        """Should drop every registered service."""
        container = Container()
        try:
            container.register("First", {"id": 1})
            container.register("Second", {"id": 2})
            assert container.service_count == 2

            container.clear()
            assert container.service_count == 0
            assert not container.contains("First")
            assert not container.contains("Second")
        finally:
            container.free()

    def test_clear_is_idempotent(self):
        """Clearing an already-empty container should succeed."""
        container = Container()
        try:
            container.clear()
            container.clear()
            assert container.service_count == 0
        finally:
            container.free()

    def test_register_after_clear(self):
        """Should be able to re-register a cleared name."""
        container = Container()
        try:
            container.register("Config", {"v": 1})
            container.clear()
            container.register("Config", {"v": 2})
            assert container.resolve("Config") == {"v": 2}
        finally:
            container.free()

    def test_clear_on_freed_container_raises(self):
        """Should raise when clearing a freed container."""
        container = Container()
        container.free()
        with pytest.raises(DIError) as exc_info:
            container.clear()
        assert exc_info.value.code == ErrorCode.INVALID_ARGUMENT


class TestLocking:
    """Tests for lock() and is_locked()."""

    def test_is_locked_false_then_true(self):
        """Should report the lock state transition."""
        container = Container()
        try:
            assert container.is_locked() is False
            container.lock()
            assert container.is_locked() is True
        finally:
            container.free()

    def test_lock_is_idempotent(self):
        """Locking twice should not raise."""
        container = Container()
        try:
            container.lock()
            container.lock()
            assert container.is_locked() is True
        finally:
            container.free()

    def test_register_after_lock_raises_locked(self):
        """register() must surface ErrorCode.LOCKED."""
        container = Container()
        try:
            container.lock()
            with pytest.raises(DIError) as exc_info:
                container.register("Late", {"nope": True})
            assert exc_info.value.code == ErrorCode.LOCKED
            assert "locked" in str(exc_info.value).lower()
        finally:
            container.free()

    def test_register_bytes_after_lock_raises_locked(self):
        """register_bytes() must surface ErrorCode.LOCKED."""
        container = Container()
        try:
            container.lock()
            with pytest.raises(DIError) as exc_info:
                container.register_bytes("Late", b"nope")
            assert exc_info.value.code == ErrorCode.LOCKED
        finally:
            container.free()

    def test_lock_beats_duplicate_registration(self):
        """A duplicate name on a locked container reports LOCKED, not
        ALREADY_REGISTERED - the lock check runs first."""
        container = Container()
        try:
            container.register("Config", {"v": 1})
            container.lock()
            with pytest.raises(DIError) as exc_info:
                container.register("Config", {"v": 2})
            assert exc_info.value.code == ErrorCode.LOCKED
        finally:
            container.free()

    def test_remove_still_works_after_lock(self):
        """Locking blocks registration only - removal stays permitted."""
        container = Container()
        try:
            container.register("Config", {"debug": True})
            container.lock()
            assert container.remove("Config") is True
            assert not container.contains("Config")
        finally:
            container.free()

    def test_clear_still_works_after_lock(self):
        """Locking blocks registration only - clearing stays permitted."""
        container = Container()
        try:
            container.register("A", {"id": 1})
            container.register("B", {"id": 2})
            container.lock()
            container.clear()
            assert container.service_count == 0
            assert container.is_locked() is True
        finally:
            container.free()

    def test_resolve_still_works_after_lock(self):
        """Reads are unaffected by locking."""
        container = Container()
        try:
            container.register("Config", {"port": 8080})
            container.lock()
            assert container.resolve("Config")["port"] == 8080
            assert container.contains("Config") is True
        finally:
            container.free()

    def test_child_scope_starts_unlocked(self):
        """Child scopes start unlocked regardless of the parent's state."""
        parent = Container()
        try:
            parent.register("Config", {"env": "prod"})
            parent.lock()
            child = parent.scope()
            try:
                assert child.is_locked() is False
                child.register("RequestId", {"id": "req-1"})
                assert child.contains("RequestId")
                # Parent stays locked
                assert parent.is_locked() is True
            finally:
                child.free()
        finally:
            parent.free()

    def test_lock_on_freed_container_raises(self):
        """Should raise when locking a freed container."""
        container = Container()
        container.free()
        with pytest.raises(DIError) as exc_info:
            container.lock()
        assert exc_info.value.code == ErrorCode.INVALID_ARGUMENT


if __name__ == "__main__":
    pytest.main([__file__, "-v"])

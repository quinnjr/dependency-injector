"""
Python bindings for the dependency-injector Rust library.

A high-performance dependency injection container with ~10ns resolution times.

Example:
    >>> from dependency_injector import Container
    >>>
    >>> container = Container()
    >>> container.register("Config", {"debug": True, "port": 8080})
    >>>
    >>> config = container.resolve("Config")
    >>> print(config["port"])  # 8080
    >>>
    >>> container.free()

Or using context manager:
    >>> from dependency_injector import Container
    >>>
    >>> with Container() as container:
    ...     container.register("Config", {"debug": True})
    ...     config = container.resolve("Config")
    ...     print(config)
"""

from .container import Container, DIError, ErrorCode, get_library_path

__all__ = ["Container", "DIError", "ErrorCode", "get_library_path"]
__version__ = "2.1.0"

"""Utility code for constructing importers, etc."""

import importlib.machinery
import sys
import types
from _typeshed import ReadableBuffer
from collections.abc import Callable
from importlib._bootstrap import module_from_spec as module_from_spec, spec_from_loader as spec_from_loader
from importlib._bootstrap_external import (
    MAGIC_NUMBER as MAGIC_NUMBER,
    cache_from_source as cache_from_source,
    decode_source as decode_source,
    source_from_cache as source_from_cache,
    spec_from_file_location as spec_from_file_location,
)
from importlib.abc import Loader
from typing_extensions import ParamSpec

_P = ParamSpec("_P")

if sys.version_info < (3, 12):
    def module_for_loader(fxn: Callable[_P, types.ModuleType]) -> Callable[_P, types.ModuleType]:
        """Decorator to handle selecting the proper module for loaders.

        The decorated function is passed the module to use instead of the module
        name. The module passed in to the function is either from sys.modules if
        it already exists or is a new module. If the module is new, then __name__
        is set the first argument to the method, __loader__ is set to self, and
        __package__ is set accordingly (if self.is_package() is defined) will be set
        before it is passed to the decorated function (if self.is_package() does
        not work for the module it will be set post-load).

        If an exception is raised and the decorator created the module it is
        subsequently removed from sys.modules.

        The decorator assumes that the decorated function takes the module name as
        the second argument.

        """

    def set_loader(fxn: Callable[_P, types.ModuleType]) -> Callable[_P, types.ModuleType]:
        """Set __loader__ on the returned module.

        This function is deprecated.

        """

    def set_package(fxn: Callable[_P, types.ModuleType]) -> Callable[_P, types.ModuleType]:
        """Set __package__ on the returned module.

        This function is deprecated.

        """

def resolve_name(name: str, package: str | None) -> str:
    """Resolve a relative module name to an absolute one."""

def find_spec(name: str, package: str | None = None) -> importlib.machinery.ModuleSpec | None:
    """Return the spec for the specified module.

    First, sys.modules is checked to see if the module was already imported. If
    so, then sys.modules[name].__spec__ is returned. If that happens to be
    set to None, then ValueError is raised. If the module is not in
    sys.modules, then sys.meta_path is searched for a suitable spec with the
    value of 'path' given to the finders. None is returned if no spec could
    be found.

    If the name is for submodule (contains a dot), the parent module is
    automatically imported.

    The name and package arguments work the same as importlib.import_module().
    In other words, relative module names (with leading dots) work.

    """

class LazyLoader(Loader):
    """A loader that creates a module which defers loading until attribute access."""

    def __init__(self, loader: Loader) -> None: ...
    @classmethod
    def factory(cls, loader: Loader) -> Callable[..., LazyLoader]:
        """Construct a callable which returns the eager loader made lazy."""

    def exec_module(self, module: types.ModuleType) -> None:
        """Make the module load lazily."""

def source_hash(source_bytes: ReadableBuffer) -> bytes:
    """Return the hash of *source_bytes* as used in hash-based pyc files."""

if sys.version_info >= (3, 14):
    __all__ = [
        "LazyLoader",
        "Loader",
        "MAGIC_NUMBER",
        "cache_from_source",
        "decode_source",
        "find_spec",
        "module_from_spec",
        "resolve_name",
        "source_from_cache",
        "source_hash",
        "spec_from_file_location",
        "spec_from_loader",
    ]

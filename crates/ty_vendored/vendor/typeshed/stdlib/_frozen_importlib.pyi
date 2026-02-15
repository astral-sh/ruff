"""Core implementation of import.

This module is NOT meant to be directly imported! It has been designed such
that it can be bootstrapped into Python as the implementation of import. As
such it requires the injection of specific modules and attributes in order to
work. One should use importlib as the public-facing version of this module.

"""

import importlib.abc
import importlib.machinery
import sys
import types
from _typeshed.importlib import LoaderProtocol
from collections.abc import Mapping, Sequence
from types import ModuleType
from typing import Any, ClassVar
from typing_extensions import deprecated

# Signature of `builtins.__import__` should be kept identical to `importlib.__import__`
def __import__(
    name: str,
    globals: Mapping[str, object] | None = None,
    locals: Mapping[str, object] | None = None,
    fromlist: Sequence[str] | None = (),
    level: int = 0,
) -> ModuleType:
    """Import a module.

    The 'globals' argument is used to infer where the import is occurring from
    to handle relative imports. The 'locals' argument is ignored. The
    'fromlist' argument specifies what should exist as attributes on the module
    being imported (e.g. ``from module import <fromlist>``).  The 'level'
    argument represents the package location to import from in a relative
    import (e.g. ``from ..pkg import mod`` would have a 'level' of 2).

    """

def spec_from_loader(
    name: str, loader: LoaderProtocol | None, *, origin: str | None = None, is_package: bool | None = None
) -> importlib.machinery.ModuleSpec | None:
    """Return a module spec based on various loader methods."""

def module_from_spec(spec: importlib.machinery.ModuleSpec) -> types.ModuleType:
    """Create a module based on the provided spec."""

def _init_module_attrs(
    spec: importlib.machinery.ModuleSpec, module: types.ModuleType, *, override: bool = False
) -> types.ModuleType: ...

class ModuleSpec:
    """The specification for a module, used for loading.

    A module's spec is the source for information about the module.  For
    data associated with the module, including source, use the spec's
    loader.

    `name` is the absolute name of the module.  `loader` is the loader
    to use when loading the module.  `parent` is the name of the
    package the module is in.  The parent is derived from the name.

    `is_package` determines if the module is considered a package or
    not.  On modules this is reflected by the `__path__` attribute.

    `origin` is the specific location used by the loader from which to
    load the module, if that information is available.  When filename is
    set, origin will match.

    `has_location` indicates that a spec's "origin" reflects a location.
    When this is True, `__file__` attribute of the module is set.

    `cached` is the location of the cached bytecode file, if any.  It
    corresponds to the `__cached__` attribute.

    `submodule_search_locations` is the sequence of path entries to
    search when importing submodules.  If set, is_package should be
    True--and False otherwise.

    Packages are simply modules that (may) have submodules.  If a spec
    has a non-None value in `submodule_search_locations`, the import
    system will consider modules loaded from the spec as packages.

    Only finders (see importlib.abc.MetaPathFinder and
    importlib.abc.PathEntryFinder) should modify ModuleSpec instances.

    """

    def __init__(
        self,
        name: str,
        loader: importlib.abc.Loader | None,
        *,
        origin: str | None = None,
        loader_state: Any = None,
        is_package: bool | None = None,
    ) -> None: ...
    name: str
    loader: importlib.abc.Loader | None
    origin: str | None
    submodule_search_locations: list[str] | None
    loader_state: Any
    cached: str | None
    @property
    def parent(self) -> str | None:
        """The name of the module's parent."""
    has_location: bool
    def __eq__(self, other: object) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

class BuiltinImporter(importlib.abc.MetaPathFinder, importlib.abc.InspectLoader):
    """Meta path import for built-in modules.

    All methods are either class or static methods to avoid the need to
    instantiate the class.

    """

    # MetaPathFinder
    if sys.version_info < (3, 12):
        @classmethod
        @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `find_spec()` instead.")
        def find_module(cls, fullname: str, path: Sequence[str] | None = None) -> importlib.abc.Loader | None:
            """Find the built-in module.

            If 'path' is ever specified then the search is considered a failure.

            This method is deprecated.  Use find_spec() instead.

            """

    @classmethod
    def find_spec(
        cls, fullname: str, path: Sequence[str] | None = None, target: types.ModuleType | None = None
    ) -> ModuleSpec | None: ...
    # InspectLoader
    @classmethod
    def is_package(cls, fullname: str) -> bool:
        """Return False as built-in modules are never packages."""

    @classmethod
    def load_module(cls, fullname: str) -> types.ModuleType:
        """Load the specified module into sys.modules and return it.

        This method is deprecated.  Use loader.exec_module() instead.

        """

    @classmethod
    def get_code(cls, fullname: str) -> None:
        """Return None as built-in modules do not have code objects."""

    @classmethod
    def get_source(cls, fullname: str) -> None:
        """Return None as built-in modules do not have source code."""
    # Loader
    if sys.version_info < (3, 12):
        @staticmethod
        @deprecated(
            "Deprecated since Python 3.4; removed in Python 3.12. "
            "The module spec is now used by the import machinery to generate a module repr."
        )
        def module_repr(module: types.ModuleType) -> str:
            """Return repr for the module.

            The method is deprecated.  The import machinery does the job itself.

            """
    if sys.version_info >= (3, 10):
        @staticmethod
        def create_module(spec: ModuleSpec) -> types.ModuleType | None:
            """Create a built-in module"""

        @staticmethod
        def exec_module(module: types.ModuleType) -> None:
            """Exec a built-in module"""
    else:
        @classmethod
        def create_module(cls, spec: ModuleSpec) -> types.ModuleType | None:
            """Create a built-in module"""

        @classmethod
        def exec_module(cls, module: types.ModuleType) -> None:
            """Exec a built-in module"""

class FrozenImporter(importlib.abc.MetaPathFinder, importlib.abc.InspectLoader):
    """Meta path import for frozen modules.

    All methods are either class or static methods to avoid the need to
    instantiate the class.

    """

    # MetaPathFinder
    if sys.version_info < (3, 12):
        @classmethod
        @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `find_spec()` instead.")
        def find_module(cls, fullname: str, path: Sequence[str] | None = None) -> importlib.abc.Loader | None:
            """Find a frozen module.

            This method is deprecated.  Use find_spec() instead.

            """

    @classmethod
    def find_spec(
        cls, fullname: str, path: Sequence[str] | None = None, target: types.ModuleType | None = None
    ) -> ModuleSpec | None: ...
    # InspectLoader
    @classmethod
    def is_package(cls, fullname: str) -> bool:
        """Return True if the frozen module is a package."""

    @classmethod
    def load_module(cls, fullname: str) -> types.ModuleType:
        """Load a frozen module.

        This method is deprecated.  Use exec_module() instead.

        """

    @classmethod
    def get_code(cls, fullname: str) -> None:
        """Return the code object for the frozen module."""

    @classmethod
    def get_source(cls, fullname: str) -> None:
        """Return None as frozen modules do not have source code."""
    # Loader
    if sys.version_info < (3, 12):
        @staticmethod
        @deprecated(
            "Deprecated since Python 3.4; removed in Python 3.12. "
            "The module spec is now used by the import machinery to generate a module repr."
        )
        def module_repr(m: types.ModuleType) -> str:
            """Return repr for the module.

            The method is deprecated.  The import machinery does the job itself.

            """
    if sys.version_info >= (3, 10):
        @staticmethod
        def create_module(spec: ModuleSpec) -> types.ModuleType | None:
            """Set __file__, if able."""
    else:
        @classmethod
        def create_module(cls, spec: ModuleSpec) -> types.ModuleType | None:
            """Use default semantics for module creation."""

    @staticmethod
    def exec_module(module: types.ModuleType) -> None: ...

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
    fromlist: Sequence[str] = (),
    level: int = 0,
) -> ModuleType: ...
def spec_from_loader(
    name: str, loader: LoaderProtocol | None, *, origin: str | None = None, is_package: bool | None = None
) -> importlib.machinery.ModuleSpec | None: ...
def module_from_spec(spec: importlib.machinery.ModuleSpec) -> types.ModuleType: ...
def _init_module_attrs(
    spec: importlib.machinery.ModuleSpec, module: types.ModuleType, *, override: bool = False
) -> types.ModuleType: ...

class ModuleSpec:
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
    def parent(self) -> str | None: ...
    has_location: bool
    def __eq__(self, other: object) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

class BuiltinImporter(importlib.abc.MetaPathFinder, importlib.abc.InspectLoader):
    # MetaPathFinder
    if sys.version_info < (3, 12):
        @classmethod
        @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `find_spec()` instead.")
        def find_module(cls, fullname: str, path: Sequence[str] | None = None) -> importlib.abc.Loader | None: ...

    @classmethod
    def find_spec(
        cls, fullname: str, path: Sequence[str] | None = None, target: types.ModuleType | None = None
    ) -> ModuleSpec | None: ...
    # InspectLoader
    @classmethod
    def is_package(cls, fullname: str) -> bool: ...
    @classmethod
    def load_module(cls, fullname: str) -> types.ModuleType: ...
    @classmethod
    def get_code(cls, fullname: str) -> None: ...
    @classmethod
    def get_source(cls, fullname: str) -> None: ...
    # Loader
    if sys.version_info < (3, 12):
        @staticmethod
        @deprecated(
            "Deprecated since Python 3.4; removed in Python 3.12. "
            "The module spec is now used by the import machinery to generate a module repr."
        )
        def module_repr(module: types.ModuleType) -> str: ...
    if sys.version_info >= (3, 10):
        @staticmethod
        def create_module(spec: ModuleSpec) -> types.ModuleType | None: ...
        @staticmethod
        def exec_module(module: types.ModuleType) -> None: ...
    else:
        @classmethod
        def create_module(cls, spec: ModuleSpec) -> types.ModuleType | None: ...
        @classmethod
        def exec_module(cls, module: types.ModuleType) -> None: ...

class FrozenImporter(importlib.abc.MetaPathFinder, importlib.abc.InspectLoader):
    # MetaPathFinder
    if sys.version_info < (3, 12):
        @classmethod
        @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `find_spec()` instead.")
        def find_module(cls, fullname: str, path: Sequence[str] | None = None) -> importlib.abc.Loader | None: ...

    @classmethod
    def find_spec(
        cls, fullname: str, path: Sequence[str] | None = None, target: types.ModuleType | None = None
    ) -> ModuleSpec | None: ...
    # InspectLoader
    @classmethod
    def is_package(cls, fullname: str) -> bool: ...
    @classmethod
    def load_module(cls, fullname: str) -> types.ModuleType: ...
    @classmethod
    def get_code(cls, fullname: str) -> None: ...
    @classmethod
    def get_source(cls, fullname: str) -> None: ...
    # Loader
    if sys.version_info < (3, 12):
        @staticmethod
        @deprecated(
            "Deprecated since Python 3.4; removed in Python 3.12. "
            "The module spec is now used by the import machinery to generate a module repr."
        )
        def module_repr(m: types.ModuleType) -> str: ...
    if sys.version_info >= (3, 10):
        @staticmethod
        def create_module(spec: ModuleSpec) -> types.ModuleType | None: ...
    else:
        @classmethod
        def create_module(cls, spec: ModuleSpec) -> types.ModuleType | None: ...

    @staticmethod
    def exec_module(module: types.ModuleType) -> None: ...

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
from types import TracebackType
from typing import Literal
from typing_extensions import ParamSpec, Self, deprecated

_P = ParamSpec("_P")

if sys.version_info < (3, 12):
    @deprecated(
        "Deprecated since Python 3.4; removed in Python 3.12. "
        "`__name__`, `__package__` and `__loader__` are now set automatically."
    )
    def module_for_loader(fxn: Callable[_P, types.ModuleType]) -> Callable[_P, types.ModuleType]: ...
    @deprecated(
        "Deprecated since Python 3.4; removed in Python 3.12. "
        "`__name__`, `__package__` and `__loader__` are now set automatically."
    )
    def set_loader(fxn: Callable[_P, types.ModuleType]) -> Callable[_P, types.ModuleType]: ...
    @deprecated(
        "Deprecated since Python 3.4; removed in Python 3.12. "
        "`__name__`, `__package__` and `__loader__` are now set automatically."
    )
    def set_package(fxn: Callable[_P, types.ModuleType]) -> Callable[_P, types.ModuleType]: ...

def resolve_name(name: str, package: str | None) -> str: ...
def find_spec(name: str, package: str | None = None) -> importlib.machinery.ModuleSpec | None: ...

class LazyLoader(Loader):
    def __init__(self, loader: Loader) -> None: ...
    @classmethod
    def factory(cls, loader: Loader) -> Callable[..., LazyLoader]: ...
    def exec_module(self, module: types.ModuleType) -> None: ...

def source_hash(source_bytes: ReadableBuffer) -> bytes: ...

if sys.version_info >= (3, 12):
    class _incompatible_extension_module_restrictions:
        def __init__(self, *, disable_check: bool) -> None: ...
        disable_check: bool
        old: Literal[-1, 0, 1]  # exists only while entered
        def __enter__(self) -> Self: ...
        def __exit__(
            self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
        ) -> None: ...
        @property
        def override(self) -> Literal[-1, 1]: ...  # undocumented

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

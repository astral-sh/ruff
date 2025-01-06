import importlib.abc
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
from typing_extensions import ParamSpec

_P = ParamSpec("_P")

if sys.version_info < (3, 12):
    def module_for_loader(fxn: Callable[_P, types.ModuleType]) -> Callable[_P, types.ModuleType]: ...
    def set_loader(fxn: Callable[_P, types.ModuleType]) -> Callable[_P, types.ModuleType]: ...
    def set_package(fxn: Callable[_P, types.ModuleType]) -> Callable[_P, types.ModuleType]: ...

def resolve_name(name: str, package: str | None) -> str: ...
def find_spec(name: str, package: str | None = None) -> importlib.machinery.ModuleSpec | None: ...

class LazyLoader(importlib.abc.Loader):
    def __init__(self, loader: importlib.abc.Loader) -> None: ...
    @classmethod
    def factory(cls, loader: importlib.abc.Loader) -> Callable[..., LazyLoader]: ...
    def exec_module(self, module: types.ModuleType) -> None: ...

def source_hash(source_bytes: ReadableBuffer) -> bytes: ...

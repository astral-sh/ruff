# Implicit protocols used in importlib.
# We intentionally omit deprecated and optional methods.

from collections.abc import Sequence
from importlib.machinery import ModuleSpec
from types import ModuleType
from typing import Protocol

__all__ = ["LoaderProtocol", "MetaPathFinderProtocol", "PathEntryFinderProtocol"]

class LoaderProtocol(Protocol):
    def load_module(self, fullname: str, /) -> ModuleType: ...

class MetaPathFinderProtocol(Protocol):
    def find_spec(self, fullname: str, path: Sequence[str] | None, target: ModuleType | None = ..., /) -> ModuleSpec | None: ...

class PathEntryFinderProtocol(Protocol):
    def find_spec(self, fullname: str, target: ModuleType | None = ..., /) -> ModuleSpec | None: ...

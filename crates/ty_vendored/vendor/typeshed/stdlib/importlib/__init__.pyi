import sys
from importlib._bootstrap import __import__ as __import__
from importlib.abc import Loader
from types import ModuleType

__all__ = ["__import__", "import_module", "invalidate_caches", "reload"]

# `importlib.import_module` return type should be kept the same as `builtins.__import__`
def import_module(name: str, package: str | None = None) -> ModuleType: ...

if sys.version_info < (3, 12):
    def find_loader(name: str, path: str | None = None) -> Loader | None: ...

def invalidate_caches() -> None: ...
def reload(module: ModuleType) -> ModuleType: ...

"""A pure Python implementation of import."""

import sys
from importlib._bootstrap import __import__ as __import__
from importlib.abc import Loader
from types import ModuleType
from typing_extensions import deprecated

__all__ = ["__import__", "import_module", "invalidate_caches", "reload"]

# `importlib.import_module` return type should be kept the same as `builtins.__import__`
def import_module(name: str, package: str | None = None) -> ModuleType:
    """Import a module.

    The 'package' argument is required when performing a relative import. It
    specifies the package to use as the anchor point from which to resolve the
    relative import to an absolute import.

    """

if sys.version_info < (3, 12):
    @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `importlib.util.find_spec()` instead.")
    def find_loader(name: str, path: str | None = None) -> Loader | None:
        """Return the loader for the specified module.

        This is a backward-compatible wrapper around find_spec().

        This function is deprecated in favor of importlib.util.find_spec().

        """

def invalidate_caches() -> None:
    """Call the invalidate_caches() method on all meta path finders stored in
    sys.meta_path (where implemented).
    """

def reload(module: ModuleType) -> ModuleType:
    """Reload the module and return it.

    The module must have been successfully imported before.

    """

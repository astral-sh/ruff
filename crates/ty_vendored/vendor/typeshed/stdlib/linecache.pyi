"""Cache lines from Python source files.

This is intended to read lines from modules imported -- hence if a filename
is not found, it will look down the module search path for a file by
that name.
"""

from collections.abc import Callable
from typing import Any
from typing_extensions import TypeAlias

__all__ = ["getline", "clearcache", "checkcache", "lazycache"]

_ModuleGlobals: TypeAlias = dict[str, Any]
_ModuleMetadata: TypeAlias = tuple[int, float | None, list[str], str]

_SourceLoader: TypeAlias = tuple[Callable[[], str | None]]

cache: dict[str, _SourceLoader | _ModuleMetadata]  # undocumented

def getline(filename: str, lineno: int, module_globals: _ModuleGlobals | None = None) -> str:
    """Get a line for a Python source file from the cache.
    Update the cache if it doesn't contain an entry for this file already.
    """

def clearcache() -> None:
    """Clear the cache entirely."""

def getlines(filename: str, module_globals: _ModuleGlobals | None = None) -> list[str]:
    """Get the lines for a Python source file from the cache.
    Update the cache if it doesn't contain an entry for this file already.
    """

def checkcache(filename: str | None = None) -> None:
    """Discard cache entries that are out of date.
    (This is not checked upon each call!)
    """

def updatecache(filename: str, module_globals: _ModuleGlobals | None = None) -> list[str]:
    """Update a cache entry and return its list of lines.
    If something's wrong, print a message, discard the cache entry,
    and return an empty list.
    """

def lazycache(filename: str, module_globals: _ModuleGlobals) -> bool:
    """Seed the cache for filename with module_globals.

    The module loader will be asked for the source only when getlines is
    called, not immediately.

    If there is an entry in the cache already, it is not altered.

    :return: True if a lazy load is registered in the cache,
        otherwise False. To register such a load a module loader with a
        get_source method must be found, the filename must be a cacheable
        filename, and the filename must not be already cached.
    """

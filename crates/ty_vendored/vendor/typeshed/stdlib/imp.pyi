"""This module provides the components needed to build your own __import__
function.  Undocumented functions are obsolete.

In most cases it is preferred you consider using the importlib module's
functionality over this module.

"""

import types
from _imp import (
    acquire_lock as acquire_lock,
    create_dynamic as create_dynamic,
    get_frozen_object as get_frozen_object,
    init_frozen as init_frozen,
    is_builtin as is_builtin,
    is_frozen as is_frozen,
    is_frozen_package as is_frozen_package,
    lock_held as lock_held,
    release_lock as release_lock,
)
from _typeshed import StrPath
from os import PathLike
from types import TracebackType
from typing import IO, Any, Final, Protocol, type_check_only

SEARCH_ERROR: Final = 0
PY_SOURCE: Final = 1
PY_COMPILED: Final = 2
C_EXTENSION: Final = 3
PY_RESOURCE: Final = 4
PKG_DIRECTORY: Final = 5
C_BUILTIN: Final = 6
PY_FROZEN: Final = 7
PY_CODERESOURCE: Final = 8
IMP_HOOK: Final = 9

def new_module(name: str) -> types.ModuleType:
    """**DEPRECATED**

    Create a new module.

    The module is not entered into sys.modules.

    """

def get_magic() -> bytes:
    """**DEPRECATED**

    Return the magic number for .pyc files.
    """

def get_tag() -> str:
    """Return the magic tag for .pyc files."""

def cache_from_source(path: StrPath, debug_override: bool | None = None) -> str:
    """**DEPRECATED**

    Given the path to a .py file, return the path to its .pyc file.

    The .py file does not need to exist; this simply returns the path to the
    .pyc file calculated as if the .py file were imported.

    If debug_override is not None, then it must be a boolean and is used in
    place of sys.flags.optimize.

    If sys.implementation.cache_tag is None then NotImplementedError is raised.

    """

def source_from_cache(path: StrPath) -> str:
    """**DEPRECATED**

    Given the path to a .pyc. file, return the path to its .py file.

    The .pyc file does not need to exist; this simply returns the path to
    the .py file calculated to correspond to the .pyc file.  If path does
    not conform to PEP 3147 format, ValueError will be raised. If
    sys.implementation.cache_tag is None then NotImplementedError is raised.

    """

def get_suffixes() -> list[tuple[str, str, int]]:
    """**DEPRECATED**"""

class NullImporter:
    """**DEPRECATED**

    Null import object.

    """

    def __init__(self, path: StrPath) -> None: ...
    def find_module(self, fullname: Any) -> None:
        """Always returns None."""

# Technically, a text file has to support a slightly different set of operations than a binary file,
# but we ignore that here.
@type_check_only
class _FileLike(Protocol):
    closed: bool
    mode: str
    def read(self) -> str | bytes: ...
    def close(self) -> Any: ...
    def __enter__(self) -> Any: ...
    def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None, /) -> Any: ...

# PathLike doesn't work for the pathname argument here
def load_source(name: str, pathname: str, file: _FileLike | None = None) -> types.ModuleType: ...
def load_compiled(name: str, pathname: str, file: _FileLike | None = None) -> types.ModuleType:
    """**DEPRECATED**"""

def load_package(name: str, path: StrPath) -> types.ModuleType:
    """**DEPRECATED**"""

def load_module(name: str, file: _FileLike | None, filename: str, details: tuple[str, str, int]) -> types.ModuleType:
    """**DEPRECATED**

    Load a module, given information returned by find_module().

    The module name must include the full package name, if any.

    """

# IO[Any] is a TextIOWrapper if name is a .py file, and a FileIO otherwise.
def find_module(
    name: str, path: None | list[str] | list[PathLike[str]] | list[StrPath] = None
) -> tuple[IO[Any], str, tuple[str, str, int]]:
    """**DEPRECATED**

    Search for a module.

    If path is omitted or None, search for a built-in, frozen or special
    module and continue search in sys.path. The module name cannot
    contain '.'; to search for a submodule of a package, pass the
    submodule name and the package's __path__.

    """

def reload(module: types.ModuleType) -> types.ModuleType:
    """**DEPRECATED**

    Reload the module and return it.

    The module must have been successfully imported before.

    """

def init_builtin(name: str) -> types.ModuleType | None:
    """**DEPRECATED**

    Load and return a built-in module by name, or None is such module doesn't
    exist
    """

def load_dynamic(name: str, path: str, file: Any = None) -> types.ModuleType:  # file argument is ignored
    """**DEPRECATED**

    Load an extension module.
    """

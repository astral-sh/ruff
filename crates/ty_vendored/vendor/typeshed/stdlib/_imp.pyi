"""(Extremely) low-level import machinery bits as used by importlib."""

import sys
import types
from _typeshed import ReadableBuffer
from importlib.machinery import ModuleSpec
from typing import Any

check_hash_based_pycs: str
if sys.version_info >= (3, 14):
    pyc_magic_number_token: int

def source_hash(key: int, source: ReadableBuffer) -> bytes: ...
def create_builtin(spec: ModuleSpec, /) -> types.ModuleType:
    """Create an extension module."""

def create_dynamic(spec: ModuleSpec, file: Any = None, /) -> types.ModuleType:
    """Create an extension module."""

def acquire_lock() -> None:
    """Acquires the interpreter's import lock for the current thread.

    This lock should be used by import hooks to ensure thread-safety when importing
    modules. On platforms without threads, this function does nothing.
    """

def exec_builtin(mod: types.ModuleType, /) -> int:
    """Initialize a built-in module."""

def exec_dynamic(mod: types.ModuleType, /) -> int:
    """Initialize an extension module."""

def extension_suffixes() -> list[str]:
    """Returns the list of file suffixes used to identify extension modules."""

def init_frozen(name: str, /) -> types.ModuleType:
    """Initializes a frozen module."""

def is_builtin(name: str, /) -> int:
    """Returns True if the module name corresponds to a built-in module."""

def is_frozen(name: str, /) -> bool:
    """Returns True if the module name corresponds to a frozen module."""

def is_frozen_package(name: str, /) -> bool:
    """Returns True if the module name is of a frozen package."""

def lock_held() -> bool:
    """Return True if the import lock is currently held, else False.

    On platforms without threads, return False.
    """

def release_lock() -> None:
    """Release the interpreter's import lock.

    On platforms without threads, this function does nothing.
    """

if sys.version_info >= (3, 11):
    def find_frozen(name: str, /, *, withdata: bool = False) -> tuple[memoryview | None, bool, str | None] | None:
        """Return info about the corresponding frozen module (if there is one) or None.

        The returned info (a 2-tuple):

         * data         the raw marshalled bytes
         * is_package   whether or not it is a package
         * origname     the originally frozen module's name, or None if not
                        a stdlib module (this will usually be the same as
                        the module's current name)
        """

    def get_frozen_object(name: str, data: ReadableBuffer | None = None, /) -> types.CodeType:
        """Create a code object for a frozen module."""

else:
    def get_frozen_object(name: str, /) -> types.CodeType:
        """Create a code object for a frozen module."""

"""Thread-local objects.

(Note that this module provides a Python version of the threading.local
 class.  Depending on the version of Python you're using, there may be a
 faster one available.  You should always import the `local` class from
 `threading`.)
"""

from threading import RLock
from typing import Any
from typing_extensions import Self, TypeAlias
from weakref import ReferenceType

__all__ = ["local"]
_LocalDict: TypeAlias = dict[Any, Any]

class _localimpl:
    """A class managing thread-local dicts"""

    __slots__ = ("key", "dicts", "localargs", "locallock", "__weakref__")
    key: str
    dicts: dict[int, tuple[ReferenceType[Any], _LocalDict]]
    # Keep localargs in sync with the *args, **kwargs annotation on local.__new__
    localargs: tuple[list[Any], dict[str, Any]]
    locallock: RLock
    def get_dict(self) -> _LocalDict:
        """Return the dict for the current thread. Raises KeyError if none
        defined.
        """

    def create_dict(self) -> _LocalDict:
        """Create a new dict for the current thread, and return it."""

class local:
    __slots__ = ("_local__impl", "__dict__")
    def __new__(cls, /, *args: Any, **kw: Any) -> Self: ...
    def __getattribute__(self, name: str) -> Any: ...
    def __setattr__(self, name: str, value: Any) -> None: ...
    def __delattr__(self, name: str) -> None: ...

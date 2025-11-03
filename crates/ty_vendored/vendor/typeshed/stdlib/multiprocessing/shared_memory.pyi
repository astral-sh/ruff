"""Provides shared memory for direct access across processes.

The API of this package is currently provisional. Refer to the
documentation for details.
"""

import sys
from collections.abc import Iterable
from types import GenericAlias
from typing import Any, Generic, TypeVar, overload
from typing_extensions import Self

__all__ = ["SharedMemory", "ShareableList"]

_SLT = TypeVar("_SLT", int, float, bool, str, bytes, None)

class SharedMemory:
    """Creates a new shared memory block or attaches to an existing
    shared memory block.

    Every shared memory block is assigned a unique name.  This enables
    one process to create a shared memory block with a particular name
    so that a different process can attach to that same shared memory
    block using that same name.

    As a resource for sharing data across processes, shared memory blocks
    may outlive the original process that created them.  When one process
    no longer needs access to a shared memory block that might still be
    needed by other processes, the close() method should be called.
    When a shared memory block is no longer needed by any process, the
    unlink() method should be called to ensure proper cleanup.
    """

    if sys.version_info >= (3, 13):
        def __init__(self, name: str | None = None, create: bool = False, size: int = 0, *, track: bool = True) -> None: ...
    else:
        def __init__(self, name: str | None = None, create: bool = False, size: int = 0) -> None: ...

    @property
    def buf(self) -> memoryview | None:
        """A memoryview of contents of the shared memory block."""

    @property
    def name(self) -> str:
        """Unique name that identifies the shared memory block."""

    @property
    def size(self) -> int:
        """Size in bytes."""

    def close(self) -> None:
        """Closes access to the shared memory from this instance but does
        not destroy the shared memory block.
        """

    def unlink(self) -> None:
        """Requests that the underlying shared memory block be destroyed.

        Unlink should be called once (and only once) across all handles
        which have access to the shared memory block, even if these
        handles belong to different processes. Closing and unlinking may
        happen in any order, but trying to access data inside a shared
        memory block after unlinking may result in memory errors,
        depending on platform.

        This method has no effect on Windows, where the only way to
        delete a shared memory block is to close all handles.
        """

    def __del__(self) -> None: ...

class ShareableList(Generic[_SLT]):
    """Pattern for a mutable list-like object shareable via a shared
    memory block.  It differs from the built-in list type in that these
    lists can not change their overall length (i.e. no append, insert,
    etc.)

    Because values are packed into a memoryview as bytes, the struct
    packing format for any storable value must require no more than 8
    characters to describe its format.
    """

    shm: SharedMemory
    @overload
    def __init__(self, sequence: None = None, *, name: str | None = None) -> None: ...
    @overload
    def __init__(self, sequence: Iterable[_SLT], *, name: str | None = None) -> None: ...
    def __getitem__(self, position: int) -> _SLT: ...
    def __setitem__(self, position: int, value: _SLT) -> None: ...
    def __reduce__(self) -> tuple[Self, tuple[_SLT, ...]]: ...
    def __len__(self) -> int: ...
    @property
    def format(self) -> str:
        """The struct packing format used by all currently stored items."""

    def count(self, value: _SLT) -> int:
        """L.count(value) -> integer -- return number of occurrences of value."""

    def index(self, value: _SLT) -> int:
        """L.index(value) -> integer -- return first index of value.
        Raises ValueError if the value is not present.
        """

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

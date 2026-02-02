"""C implementation of the Python queue module.
This module is an implementation detail, please do not use it directly.
"""

from types import GenericAlias
from typing import Any, Generic, TypeVar
from typing_extensions import disjoint_base

_T = TypeVar("_T")

class Empty(Exception):
    """Exception raised by Queue.get(block=0)/get_nowait()."""

@disjoint_base
class SimpleQueue(Generic[_T]):
    """Simple, unbounded, reentrant FIFO queue."""

    def __init__(self) -> None: ...
    def empty(self) -> bool:
        """Return True if the queue is empty, False otherwise (not reliable!)."""

    def get(self, block: bool = True, timeout: float | None = None) -> _T:
        """Remove and return an item from the queue.

        If optional args 'block' is true and 'timeout' is None (the default),
        block if necessary until an item is available. If 'timeout' is
        a non-negative number, it blocks at most 'timeout' seconds and raises
        the Empty exception if no item was available within that time.
        Otherwise ('block' is false), return an item if one is immediately
        available, else raise the Empty exception ('timeout' is ignored
        in that case).
        """

    def get_nowait(self) -> _T:
        """Remove and return an item from the queue without blocking.

        Only get an item if one is immediately available. Otherwise
        raise the Empty exception.
        """

    def put(self, item: _T, block: bool = True, timeout: float | None = None) -> None:
        """Put the item on the queue.

        The optional 'block' and 'timeout' arguments are ignored, as this method
        never blocks.  They are provided for compatibility with the Queue class.
        """

    def put_nowait(self, item: _T) -> None:
        """Put an item into the queue without blocking.

        This is exactly equivalent to `put(item)` and is only provided
        for compatibility with the Queue class.
        """

    def qsize(self) -> int:
        """Return the approximate size of the queue (not reliable!)."""

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

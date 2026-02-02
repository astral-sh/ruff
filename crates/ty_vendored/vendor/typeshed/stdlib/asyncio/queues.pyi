import sys
from _typeshed import SupportsRichComparisonT
from asyncio.events import AbstractEventLoop
from types import GenericAlias
from typing import Any, Generic, TypeVar

if sys.version_info >= (3, 10):
    from .mixins import _LoopBoundMixin
else:
    _LoopBoundMixin = object

class QueueEmpty(Exception):
    """Raised when Queue.get_nowait() is called on an empty Queue."""

class QueueFull(Exception):
    """Raised when the Queue.put_nowait() method is called on a full Queue."""

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.version_info >= (3, 13):
    __all__ = ("Queue", "PriorityQueue", "LifoQueue", "QueueFull", "QueueEmpty", "QueueShutDown")

else:
    __all__ = ("Queue", "PriorityQueue", "LifoQueue", "QueueFull", "QueueEmpty")

_T = TypeVar("_T")

if sys.version_info >= (3, 13):
    class QueueShutDown(Exception):
        """Raised when putting on to or getting from a shut-down Queue."""

# If Generic[_T] is last and _LoopBoundMixin is object, pyright is unhappy.
# We can remove the noqa pragma when dropping 3.9 support.
class Queue(Generic[_T], _LoopBoundMixin):  # noqa: Y059
    """A queue, useful for coordinating producer and consumer coroutines.

    If maxsize is less than or equal to zero, the queue size is infinite. If it
    is an integer greater than 0, then "await put()" will block when the
    queue reaches maxsize, until an item is removed by get().

    Unlike the standard library Queue, you can reliably know this Queue's size
    with qsize(), since your single-threaded asyncio application won't be
    interrupted between calling qsize() and doing an operation on the Queue.
    """

    if sys.version_info >= (3, 10):
        def __init__(self, maxsize: int = 0) -> None: ...
    else:
        def __init__(self, maxsize: int = 0, *, loop: AbstractEventLoop | None = None) -> None: ...

    def _init(self, maxsize: int) -> None: ...
    def _get(self) -> _T: ...
    def _put(self, item: _T) -> None: ...
    def _format(self) -> str: ...
    def qsize(self) -> int:
        """Number of items in the queue."""

    @property
    def maxsize(self) -> int:
        """Number of items allowed in the queue."""

    def empty(self) -> bool:
        """Return True if the queue is empty, False otherwise."""

    def full(self) -> bool:
        """Return True if there are maxsize items in the queue.

        Note: if the Queue was initialized with maxsize=0 (the default),
        then full() is never True.
        """

    async def put(self, item: _T) -> None:
        """Put an item into the queue.

        Put an item into the queue. If the queue is full, wait until a free
        slot is available before adding item.

        Raises QueueShutDown if the queue has been shut down.
        """

    def put_nowait(self, item: _T) -> None:
        """Put an item into the queue without blocking.

        If no free slot is immediately available, raise QueueFull.

        Raises QueueShutDown if the queue has been shut down.
        """

    async def get(self) -> _T:
        """Remove and return an item from the queue.

        If queue is empty, wait until an item is available.

        Raises QueueShutDown if the queue has been shut down and is empty, or
        if the queue has been shut down immediately.
        """

    def get_nowait(self) -> _T:
        """Remove and return an item from the queue.

        Return an item if one is immediately available, else raise QueueEmpty.

        Raises QueueShutDown if the queue has been shut down and is empty, or
        if the queue has been shut down immediately.
        """

    async def join(self) -> None:
        """Block until all items in the queue have been gotten and processed.

        The count of unfinished tasks goes up whenever an item is added to the
        queue. The count goes down whenever a consumer calls task_done() to
        indicate that the item was retrieved and all work on it is complete.
        When the count of unfinished tasks drops to zero, join() unblocks.
        """

    def task_done(self) -> None:
        """Indicate that a formerly enqueued task is complete.

        Used by queue consumers. For each get() used to fetch a task,
        a subsequent call to task_done() tells the queue that the processing
        on the task is complete.

        If a join() is currently blocking, it will resume when all items have
        been processed (meaning that a task_done() call was received for every
        item that had been put() into the queue).

        Raises ValueError if called more times than there were items placed in
        the queue.
        """

    def __class_getitem__(cls, type: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """
    if sys.version_info >= (3, 13):
        def shutdown(self, immediate: bool = False) -> None:
            """Shut-down the queue, making queue gets and puts raise QueueShutDown.

            By default, gets will only raise once the queue is empty. Set
            'immediate' to True to make gets raise immediately instead.

            All blocked callers of put() and get() will be unblocked.

            If 'immediate', the queue is drained and unfinished tasks
            is reduced by the number of drained tasks.  If unfinished tasks
            is reduced to zero, callers of Queue.join are unblocked.
            """

class PriorityQueue(Queue[SupportsRichComparisonT]):
    """A subclass of Queue; retrieves entries in priority order (lowest first).

    Entries are typically tuples of the form: (priority number, data).
    """

class LifoQueue(Queue[_T]):
    """A subclass of Queue that retrieves most recently added entries first."""

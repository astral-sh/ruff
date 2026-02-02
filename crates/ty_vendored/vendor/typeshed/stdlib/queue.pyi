"""A multi-producer, multi-consumer queue."""

import sys
from _queue import Empty as Empty, SimpleQueue as SimpleQueue
from _typeshed import SupportsRichComparisonT
from threading import Condition, Lock
from types import GenericAlias
from typing import Any, Generic, TypeVar

__all__ = ["Empty", "Full", "Queue", "PriorityQueue", "LifoQueue", "SimpleQueue"]
if sys.version_info >= (3, 13):
    __all__ += ["ShutDown"]

_T = TypeVar("_T")

class Full(Exception):
    """Exception raised by Queue.put(block=0)/put_nowait()."""

if sys.version_info >= (3, 13):
    class ShutDown(Exception):
        """Raised when put/get with shut-down queue."""

class Queue(Generic[_T]):
    """Create a queue object with a given maximum size.

    If maxsize is <= 0, the queue size is infinite.
    """

    maxsize: int

    mutex: Lock  # undocumented
    not_empty: Condition  # undocumented
    not_full: Condition  # undocumented
    all_tasks_done: Condition  # undocumented
    unfinished_tasks: int  # undocumented
    if sys.version_info >= (3, 13):
        is_shutdown: bool  # undocumented
    # Despite the fact that `queue` has `deque` type,
    # we treat it as `Any` to allow different implementations in subtypes.
    queue: Any  # undocumented
    def __init__(self, maxsize: int = 0) -> None: ...
    def _init(self, maxsize: int) -> None: ...
    def empty(self) -> bool:
        """Return True if the queue is empty, False otherwise (not reliable!).

        This method is likely to be removed at some point.  Use qsize() == 0
        as a direct substitute, but be aware that either approach risks a race
        condition where a queue can grow before the result of empty() or
        qsize() can be used.

        To create code that needs to wait for all queued tasks to be
        completed, the preferred technique is to use the join() method.
        """

    def full(self) -> bool:
        """Return True if the queue is full, False otherwise (not reliable!).

        This method is likely to be removed at some point.  Use qsize() >= n
        as a direct substitute, but be aware that either approach risks a race
        condition where a queue can shrink before the result of full() or
        qsize() can be used.
        """

    def get(self, block: bool = True, timeout: float | None = None) -> _T:
        """Remove and return an item from the queue.

        If optional args 'block' is true and 'timeout' is None (the default),
        block if necessary until an item is available. If 'timeout' is
        a non-negative number, it blocks at most 'timeout' seconds and raises
        the Empty exception if no item was available within that time.
        Otherwise ('block' is false), return an item if one is immediately
        available, else raise the Empty exception ('timeout' is ignored
        in that case).

        Raises ShutDown if the queue has been shut down and is empty,
        or if the queue has been shut down immediately.
        """

    def get_nowait(self) -> _T:
        """Remove and return an item from the queue without blocking.

        Only get an item if one is immediately available. Otherwise
        raise the Empty exception.
        """
    if sys.version_info >= (3, 13):
        def shutdown(self, immediate: bool = False) -> None:
            """Shut-down the queue, making queue gets and puts raise ShutDown.

            By default, gets will only raise once the queue is empty. Set
            'immediate' to True to make gets raise immediately instead.

            All blocked callers of put() and get() will be unblocked.

            If 'immediate', the queue is drained and unfinished tasks
            is reduced by the number of drained tasks.  If unfinished tasks
            is reduced to zero, callers of Queue.join are unblocked.
            """

    def _get(self) -> _T: ...
    def put(self, item: _T, block: bool = True, timeout: float | None = None) -> None:
        """Put an item into the queue.

        If optional args 'block' is true and 'timeout' is None (the default),
        block if necessary until a free slot is available. If 'timeout' is
        a non-negative number, it blocks at most 'timeout' seconds and raises
        the Full exception if no free slot was available within that time.
        Otherwise ('block' is false), put an item on the queue if a free slot
        is immediately available, else raise the Full exception ('timeout'
        is ignored in that case).

        Raises ShutDown if the queue has been shut down.
        """

    def put_nowait(self, item: _T) -> None:
        """Put an item into the queue without blocking.

        Only enqueue the item if a free slot is immediately available.
        Otherwise raise the Full exception.
        """

    def _put(self, item: _T) -> None: ...
    def join(self) -> None:
        """Blocks until all items in the Queue have been gotten and processed.

        The count of unfinished tasks goes up whenever an item is added to the
        queue. The count goes down whenever a consumer thread calls task_done()
        to indicate the item was retrieved and all work on it is complete.

        When the count of unfinished tasks drops to zero, join() unblocks.
        """

    def qsize(self) -> int:
        """Return the approximate size of the queue (not reliable!)."""

    def _qsize(self) -> int: ...
    def task_done(self) -> None:
        """Indicate that a formerly enqueued task is complete.

        Used by Queue consumer threads.  For each get() used to fetch a task,
        a subsequent call to task_done() tells the queue that the processing
        on the task is complete.

        If a join() is currently blocking, it will resume when all items
        have been processed (meaning that a task_done() call was received
        for every item that had been put() into the queue).

        Raises a ValueError if called more times than there were items
        placed in the queue.
        """

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

class PriorityQueue(Queue[SupportsRichComparisonT]):
    """Variant of Queue that retrieves open entries in priority order (lowest first).

    Entries are typically tuples of the form:  (priority number, data).
    """

    queue: list[SupportsRichComparisonT]

class LifoQueue(Queue[_T]):
    """Variant of Queue that retrieves most recently added entries first."""

    queue: list[_T]

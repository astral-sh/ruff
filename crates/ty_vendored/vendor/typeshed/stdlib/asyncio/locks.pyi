"""Synchronization primitives."""

import enum
import sys
from _typeshed import Unused
from collections import deque
from collections.abc import Callable
from types import TracebackType
from typing import Any, Literal, TypeVar
from typing_extensions import Self

from .events import AbstractEventLoop
from .futures import Future

if sys.version_info >= (3, 10):
    from .mixins import _LoopBoundMixin
else:
    _LoopBoundMixin = object

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.version_info >= (3, 11):
    __all__ = ("Lock", "Event", "Condition", "Semaphore", "BoundedSemaphore", "Barrier")
else:
    __all__ = ("Lock", "Event", "Condition", "Semaphore", "BoundedSemaphore")

_T = TypeVar("_T")

class _ContextManagerMixin:
    async def __aenter__(self) -> None: ...
    async def __aexit__(
        self, exc_type: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None
    ) -> None: ...

class Lock(_ContextManagerMixin, _LoopBoundMixin):
    """Primitive lock objects.

    A primitive lock is a synchronization primitive that is not owned
    by a particular task when locked.  A primitive lock is in one
    of two states, 'locked' or 'unlocked'.

    It is created in the unlocked state.  It has two basic methods,
    acquire() and release().  When the state is unlocked, acquire()
    changes the state to locked and returns immediately.  When the
    state is locked, acquire() blocks until a call to release() in
    another task changes it to unlocked, then the acquire() call
    resets it to locked and returns.  The release() method should only
    be called in the locked state; it changes the state to unlocked
    and returns immediately.  If an attempt is made to release an
    unlocked lock, a RuntimeError will be raised.

    When more than one task is blocked in acquire() waiting for
    the state to turn to unlocked, only one task proceeds when a
    release() call resets the state to unlocked; successive release()
    calls will unblock tasks in FIFO order.

    Locks also support the asynchronous context management protocol.
    'async with lock' statement should be used.

    Usage:

        lock = Lock()
        ...
        await lock.acquire()
        try:
            ...
        finally:
            lock.release()

    Context manager usage:

        lock = Lock()
        ...
        async with lock:
             ...

    Lock objects can be tested for locking state:

        if not lock.locked():
           await lock.acquire()
        else:
           # lock is acquired
           ...

    """

    _waiters: deque[Future[Any]] | None
    if sys.version_info >= (3, 10):
        def __init__(self) -> None: ...
    else:
        def __init__(self, *, loop: AbstractEventLoop | None = None) -> None: ...

    def locked(self) -> bool:
        """Return True if lock is acquired."""

    async def acquire(self) -> Literal[True]:
        """Acquire a lock.

        This method blocks until the lock is unlocked, then sets it to
        locked and returns True.
        """

    def release(self) -> None:
        """Release a lock.

        When the lock is locked, reset it to unlocked, and return.
        If any other tasks are blocked waiting for the lock to become
        unlocked, allow exactly one of them to proceed.

        When invoked on an unlocked lock, a RuntimeError is raised.

        There is no return value.
        """

class Event(_LoopBoundMixin):
    """Asynchronous equivalent to threading.Event.

    Class implementing event objects. An event manages a flag that can be set
    to true with the set() method and reset to false with the clear() method.
    The wait() method blocks until the flag is true. The flag is initially
    false.
    """

    _waiters: deque[Future[Any]]
    if sys.version_info >= (3, 10):
        def __init__(self) -> None: ...
    else:
        def __init__(self, *, loop: AbstractEventLoop | None = None) -> None: ...

    def is_set(self) -> bool:
        """Return True if and only if the internal flag is true."""

    def set(self) -> None:
        """Set the internal flag to true. All tasks waiting for it to
        become true are awakened. Tasks that call wait() once the flag is
        true will not block at all.
        """

    def clear(self) -> None:
        """Reset the internal flag to false. Subsequently, tasks calling
        wait() will block until set() is called to set the internal flag
        to true again.
        """

    async def wait(self) -> Literal[True]:
        """Block until the internal flag is true.

        If the internal flag is true on entry, return True
        immediately.  Otherwise, block until another task calls
        set() to set the flag to true, then return True.
        """

class Condition(_ContextManagerMixin, _LoopBoundMixin):
    """Asynchronous equivalent to threading.Condition.

    This class implements condition variable objects. A condition variable
    allows one or more tasks to wait until they are notified by another
    task.

    A new Lock object is created and used as the underlying lock.
    """

    _waiters: deque[Future[Any]]
    if sys.version_info >= (3, 10):
        def __init__(self, lock: Lock | None = None) -> None: ...
    else:
        def __init__(self, lock: Lock | None = None, *, loop: AbstractEventLoop | None = None) -> None: ...

    def locked(self) -> bool: ...
    async def acquire(self) -> Literal[True]: ...
    def release(self) -> None: ...
    async def wait(self) -> Literal[True]:
        """Wait until notified.

        If the calling task has not acquired the lock when this
        method is called, a RuntimeError is raised.

        This method releases the underlying lock, and then blocks
        until it is awakened by a notify() or notify_all() call for
        the same condition variable in another task.  Once
        awakened, it re-acquires the lock and returns True.

        This method may return spuriously,
        which is why the caller should always
        re-check the state and be prepared to wait() again.
        """

    async def wait_for(self, predicate: Callable[[], _T]) -> _T:
        """Wait until a predicate becomes true.

        The predicate should be a callable whose result will be
        interpreted as a boolean value.  The method will repeatedly
        wait() until it evaluates to true.  The final predicate value is
        the return value.
        """

    def notify(self, n: int = 1) -> None:
        """By default, wake up one task waiting on this condition, if any.
        If the calling task has not acquired the lock when this method
        is called, a RuntimeError is raised.

        This method wakes up n of the tasks waiting for the condition
         variable; if fewer than n are waiting, they are all awoken.

        Note: an awakened task does not actually return from its
        wait() call until it can reacquire the lock. Since notify() does
        not release the lock, its caller should.
        """

    def notify_all(self) -> None:
        """Wake up all tasks waiting on this condition. This method acts
        like notify(), but wakes up all waiting tasks instead of one. If the
        calling task has not acquired the lock when this method is called,
        a RuntimeError is raised.
        """

class Semaphore(_ContextManagerMixin, _LoopBoundMixin):
    """A Semaphore implementation.

    A semaphore manages an internal counter which is decremented by each
    acquire() call and incremented by each release() call. The counter
    can never go below zero; when acquire() finds that it is zero, it blocks,
    waiting until some other thread calls release().

    Semaphores also support the context management protocol.

    The optional argument gives the initial value for the internal
    counter; it defaults to 1. If the value given is less than 0,
    ValueError is raised.
    """

    _value: int
    _waiters: deque[Future[Any]] | None
    if sys.version_info >= (3, 10):
        def __init__(self, value: int = 1) -> None: ...
    else:
        def __init__(self, value: int = 1, *, loop: AbstractEventLoop | None = None) -> None: ...

    def locked(self) -> bool:
        """Returns True if semaphore cannot be acquired immediately."""

    async def acquire(self) -> Literal[True]:
        """Acquire a semaphore.

        If the internal counter is larger than zero on entry,
        decrement it by one and return True immediately.  If it is
        zero on entry, block, waiting until some other task has
        called release() to make it larger than 0, and then return
        True.
        """

    def release(self) -> None:
        """Release a semaphore, incrementing the internal counter by one.

        When it was zero on entry and another task is waiting for it to
        become larger than zero again, wake up that task.
        """

    def _wake_up_next(self) -> None:
        """Wake up the first waiter that isn't done."""

class BoundedSemaphore(Semaphore):
    """A bounded semaphore implementation.

    This raises ValueError in release() if it would increase the value
    above the initial value.
    """

if sys.version_info >= (3, 11):
    class _BarrierState(enum.Enum):  # undocumented
        FILLING = "filling"
        DRAINING = "draining"
        RESETTING = "resetting"
        BROKEN = "broken"

    class Barrier(_LoopBoundMixin):
        """Asyncio equivalent to threading.Barrier

        Implements a Barrier primitive.
        Useful for synchronizing a fixed number of tasks at known synchronization
        points. Tasks block on 'wait()' and are simultaneously awoken once they
        have all made their call.
        """

        def __init__(self, parties: int) -> None:
            """Create a barrier, initialised to 'parties' tasks."""

        async def __aenter__(self) -> Self: ...
        async def __aexit__(self, *args: Unused) -> None: ...
        async def wait(self) -> int:
            """Wait for the barrier.

            When the specified number of tasks have started waiting, they are all
            simultaneously awoken.
            Returns an unique and individual index number from 0 to 'parties-1'.
            """

        async def abort(self) -> None:
            """Place the barrier into a 'broken' state.

            Useful in case of error.  Any currently waiting tasks and tasks
            attempting to 'wait()' will have BrokenBarrierError raised.
            """

        async def reset(self) -> None:
            """Reset the barrier to the initial state.

            Any tasks currently waiting will get the BrokenBarrier exception
            raised.
            """

        @property
        def parties(self) -> int:
            """Return the number of tasks required to trip the barrier."""

        @property
        def n_waiting(self) -> int:
            """Return the number of tasks currently waiting at the barrier."""

        @property
        def broken(self) -> bool:
            """Return True if the barrier is in a broken state."""

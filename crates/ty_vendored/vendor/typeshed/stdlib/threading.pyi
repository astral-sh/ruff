"""Thread module emulating a subset of Java's threading model."""

import _thread
import sys
from _thread import _ExceptHookArgs, get_native_id as get_native_id
from _typeshed import ProfileFunction, TraceFunction
from collections.abc import Callable, Iterable, Mapping
from contextvars import ContextVar
from types import TracebackType
from typing import Any, Final, TypeVar, final
from typing_extensions import deprecated

_T = TypeVar("_T")

__all__ = [
    "get_ident",
    "active_count",
    "Condition",
    "current_thread",
    "enumerate",
    "main_thread",
    "TIMEOUT_MAX",
    "Event",
    "Lock",
    "RLock",
    "Semaphore",
    "BoundedSemaphore",
    "Thread",
    "Barrier",
    "BrokenBarrierError",
    "Timer",
    "ThreadError",
    "ExceptHookArgs",
    "setprofile",
    "settrace",
    "local",
    "stack_size",
    "excepthook",
    "get_native_id",
]

if sys.version_info >= (3, 10):
    __all__ += ["getprofile", "gettrace"]

if sys.version_info >= (3, 12):
    __all__ += ["setprofile_all_threads", "settrace_all_threads"]

_profile_hook: ProfileFunction | None

def active_count() -> int:
    """Return the number of Thread objects currently alive.

    The returned count is equal to the length of the list returned by
    enumerate().

    """

@deprecated("Deprecated since Python 3.10. Use `active_count()` instead.")
def activeCount() -> int:
    """Return the number of Thread objects currently alive.

    This function is deprecated, use active_count() instead.

    """

def current_thread() -> Thread:
    """Return the current Thread object, corresponding to the caller's thread of control.

    If the caller's thread of control was not created through the threading
    module, a dummy thread object with limited functionality is returned.

    """

@deprecated("Deprecated since Python 3.10. Use `current_thread()` instead.")
def currentThread() -> Thread:
    """Return the current Thread object, corresponding to the caller's thread of control.

    This function is deprecated, use current_thread() instead.

    """

def get_ident() -> int:
    """Return a non-zero integer that uniquely identifies the current thread
    amongst other threads that exist simultaneously.
    This may be used to identify per-thread resources.
    Even though on some platforms threads identities may appear to be
    allocated consecutive numbers starting at 1, this behavior should not
    be relied upon, and the number should be seen purely as a magic cookie.
    A thread's identity may be reused for another thread after it exits.
    """

def enumerate() -> list[Thread]:
    """Return a list of all Thread objects currently alive.

    The list includes daemonic threads, dummy thread objects created by
    current_thread(), and the main thread. It excludes terminated threads and
    threads that have not yet been started.

    """

def main_thread() -> Thread:
    """Return the main thread object.

    In normal conditions, the main thread is the thread from which the
    Python interpreter was started.
    """

def settrace(func: TraceFunction) -> None:
    """Set a trace function for all threads started from the threading module.

    The func will be passed to sys.settrace() for each thread, before its run()
    method is called.
    """

def setprofile(func: ProfileFunction | None) -> None:
    """Set a profile function for all threads started from the threading module.

    The func will be passed to sys.setprofile() for each thread, before its
    run() method is called.
    """

if sys.version_info >= (3, 12):
    def setprofile_all_threads(func: ProfileFunction | None) -> None:
        """Set a profile function for all threads started from the threading module
        and all Python threads that are currently executing.

        The func will be passed to sys.setprofile() for each thread, before its
        run() method is called.
        """

    def settrace_all_threads(func: TraceFunction) -> None:
        """Set a trace function for all threads started from the threading module
        and all Python threads that are currently executing.

        The func will be passed to sys.settrace() for each thread, before its run()
        method is called.
        """

if sys.version_info >= (3, 10):
    def gettrace() -> TraceFunction | None:
        """Get the trace function as set by threading.settrace()."""

    def getprofile() -> ProfileFunction | None:
        """Get the profiler function as set by threading.setprofile()."""

def stack_size(size: int = 0, /) -> int:
    """Return the thread stack size used when creating new threads.  The
    optional size argument specifies the stack size (in bytes) to be used
    for subsequently created threads, and must be 0 (use platform or
    configured default) or a positive integer value of at least 32,768 (32k).
    If changing the thread stack size is unsupported, a ThreadError
    exception is raised.  If the specified size is invalid, a ValueError
    exception is raised, and the stack size is unmodified.  32k bytes
     currently the minimum supported stack size value to guarantee
    sufficient stack space for the interpreter itself.

    Note that some platforms may have particular restrictions on values for
    the stack size, such as requiring a minimum stack size larger than 32 KiB or
    requiring allocation in multiples of the system memory page size
    - platform documentation should be referred to for more information
    (4 KiB pages are common; using multiples of 4096 for the stack size is
    the suggested approach in the absence of more specific information).
    """

TIMEOUT_MAX: Final[float]

ThreadError = _thread.error
local = _thread._local

class Thread:
    """A class that represents a thread of control.

    This class can be safely subclassed in a limited fashion. There are two ways
    to specify the activity: by passing a callable object to the constructor, or
    by overriding the run() method in a subclass.

    """

    name: str
    @property
    def ident(self) -> int | None:
        """Thread identifier of this thread or None if it has not been started.

        This is a nonzero integer. See the get_ident() function. Thread
        identifiers may be recycled when a thread exits and another thread is
        created. The identifier is available even after the thread has exited.

        """
    daemon: bool
    if sys.version_info >= (3, 14):
        def __init__(
            self,
            group: None = None,
            target: Callable[..., object] | None = None,
            name: str | None = None,
            args: Iterable[Any] = (),
            kwargs: Mapping[str, Any] | None = None,
            *,
            daemon: bool | None = None,
            context: ContextVar[Any] | None = None,
        ) -> None:
            """This constructor should always be called with keyword arguments. Arguments are:

            *group* should be None; reserved for future extension when a ThreadGroup
            class is implemented.

            *target* is the callable object to be invoked by the run()
            method. Defaults to None, meaning nothing is called.

            *name* is the thread name. By default, a unique name is constructed of
            the form "Thread-N" where N is a small decimal number.

            *args* is a list or tuple of arguments for the target invocation. Defaults to ().

            *kwargs* is a dictionary of keyword arguments for the target
            invocation. Defaults to {}.

            *context* is the contextvars.Context value to use for the thread.
            The default value is None, which means to check
            sys.flags.thread_inherit_context.  If that flag is true, use a copy
            of the context of the caller.  If false, use an empty context.  To
            explicitly start with an empty context, pass a new instance of
            contextvars.Context().  To explicitly start with a copy of the current
            context, pass the value from contextvars.copy_context().

            If a subclass overrides the constructor, it must make sure to invoke
            the base class constructor (Thread.__init__()) before doing anything
            else to the thread.

            """
    else:
        def __init__(
            self,
            group: None = None,
            target: Callable[..., object] | None = None,
            name: str | None = None,
            args: Iterable[Any] = (),
            kwargs: Mapping[str, Any] | None = None,
            *,
            daemon: bool | None = None,
        ) -> None:
            """This constructor should always be called with keyword arguments. Arguments are:

            *group* should be None; reserved for future extension when a ThreadGroup
            class is implemented.

            *target* is the callable object to be invoked by the run()
            method. Defaults to None, meaning nothing is called.

            *name* is the thread name. By default, a unique name is constructed of
            the form "Thread-N" where N is a small decimal number.

            *args* is a list or tuple of arguments for the target invocation. Defaults to ().

            *kwargs* is a dictionary of keyword arguments for the target
            invocation. Defaults to {}.

            If a subclass overrides the constructor, it must make sure to invoke
            the base class constructor (Thread.__init__()) before doing anything
            else to the thread.

            """

    def start(self) -> None:
        """Start the thread's activity.

        It must be called at most once per thread object. It arranges for the
        object's run() method to be invoked in a separate thread of control.

        This method will raise a RuntimeError if called more than once on the
        same thread object.

        """

    def run(self) -> None:
        """Method representing the thread's activity.

        You may override this method in a subclass. The standard run() method
        invokes the callable object passed to the object's constructor as the
        target argument, if any, with sequential and keyword arguments taken
        from the args and kwargs arguments, respectively.

        """

    def join(self, timeout: float | None = None) -> None:
        """Wait until the thread terminates.

        This blocks the calling thread until the thread whose join() method is
        called terminates -- either normally or through an unhandled exception
        or until the optional timeout occurs.

        When the timeout argument is present and not None, it should be a
        floating-point number specifying a timeout for the operation in seconds
        (or fractions thereof). As join() always returns None, you must call
        is_alive() after join() to decide whether a timeout happened -- if the
        thread is still alive, the join() call timed out.

        When the timeout argument is not present or None, the operation will
        block until the thread terminates.

        A thread can be join()ed many times.

        join() raises a RuntimeError if an attempt is made to join the current
        thread as that would cause a deadlock. It is also an error to join() a
        thread before it has been started and attempts to do so raises the same
        exception.

        """

    @property
    def native_id(self) -> int | None:  # only available on some platforms
        """Native integral thread ID of this thread, or None if it has not been started.

        This is a non-negative integer. See the get_native_id() function.
        This represents the Thread ID as reported by the kernel.

        """

    def is_alive(self) -> bool:
        """Return whether the thread is alive.

        This method returns True just before the run() method starts until just
        after the run() method terminates. See also the module function
        enumerate().

        """

    @deprecated("Deprecated since Python 3.10. Read the `daemon` attribute instead.")
    def isDaemon(self) -> bool:
        """Return whether this thread is a daemon.

        This method is deprecated, use the daemon attribute instead.

        """

    @deprecated("Deprecated since Python 3.10. Set the `daemon` attribute instead.")
    def setDaemon(self, daemonic: bool) -> None:
        """Set whether this thread is a daemon.

        This method is deprecated, use the .daemon property instead.

        """

    @deprecated("Deprecated since Python 3.10. Read the `name` attribute instead.")
    def getName(self) -> str:
        """Return a string used for identification purposes only.

        This method is deprecated, use the name attribute instead.

        """

    @deprecated("Deprecated since Python 3.10. Set the `name` attribute instead.")
    def setName(self, name: str) -> None:
        """Set the name string for this thread.

        This method is deprecated, use the name attribute instead.

        """

class _DummyThread(Thread):
    def __init__(self) -> None: ...

# This is actually the function _thread.allocate_lock for <= 3.12
Lock = _thread.LockType

# Python implementation of RLock.
@final
class _RLock:
    """This class implements reentrant lock objects.

    A reentrant lock must be released by the thread that acquired it. Once a
    thread has acquired a reentrant lock, the same thread may acquire it
    again without blocking; the thread must release it once for each time it
    has acquired it.

    """

    _count: int
    def acquire(self, blocking: bool = True, timeout: float = -1) -> bool:
        """Acquire a lock, blocking or non-blocking.

        When invoked without arguments: if this thread already owns the lock,
        increment the recursion level by one, and return immediately. Otherwise,
        if another thread owns the lock, block until the lock is unlocked. Once
        the lock is unlocked (not owned by any thread), then grab ownership, set
        the recursion level to one, and return. If more than one thread is
        blocked waiting until the lock is unlocked, only one at a time will be
        able to grab ownership of the lock. There is no return value in this
        case.

        When invoked with the blocking argument set to true, do the same thing
        as when called without arguments, and return true.

        When invoked with the blocking argument set to false, do not block. If a
        call without an argument would block, return false immediately;
        otherwise, do the same thing as when called without arguments, and
        return true.

        When invoked with the floating-point timeout argument set to a positive
        value, block for at most the number of seconds specified by timeout
        and as long as the lock cannot be acquired.  Return true if the lock has
        been acquired, false if the timeout has elapsed.

        """

    def release(self) -> None:
        """Release a lock, decrementing the recursion level.

        If after the decrement it is zero, reset the lock to unlocked (not owned
        by any thread), and if any other threads are blocked waiting for the
        lock to become unlocked, allow exactly one of them to proceed. If after
        the decrement the recursion level is still nonzero, the lock remains
        locked and owned by the calling thread.

        Only call this method when the calling thread owns the lock. A
        RuntimeError is raised if this method is called when the lock is
        unlocked.

        There is no return value.

        """
    __enter__ = acquire
    def __exit__(self, t: type[BaseException] | None, v: BaseException | None, tb: TracebackType | None) -> None: ...

    if sys.version_info >= (3, 14):
        def locked(self) -> bool:
            """Return whether this object is locked."""

RLock = _thread.RLock  # Actually a function at runtime.

class Condition:
    """Class that implements a condition variable.

    A condition variable allows one or more threads to wait until they are
    notified by another thread.

    If the lock argument is given and not None, it must be a Lock or RLock
    object, and it is used as the underlying lock. Otherwise, a new RLock object
    is created and used as the underlying lock.

    """

    def __init__(self, lock: Lock | _RLock | RLock | None = None) -> None: ...
    def __enter__(self) -> bool: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> None: ...
    def acquire(self, blocking: bool = True, timeout: float = -1) -> bool: ...
    def release(self) -> None: ...
    if sys.version_info >= (3, 14):
        def locked(self) -> bool: ...

    def wait(self, timeout: float | None = None) -> bool:
        """Wait until notified or until a timeout occurs.

        If the calling thread has not acquired the lock when this method is
        called, a RuntimeError is raised.

        This method releases the underlying lock, and then blocks until it is
        awakened by a notify() or notify_all() call for the same condition
        variable in another thread, or until the optional timeout occurs. Once
        awakened or timed out, it re-acquires the lock and returns.

        When the timeout argument is present and not None, it should be a
        floating-point number specifying a timeout for the operation in seconds
        (or fractions thereof).

        When the underlying lock is an RLock, it is not released using its
        release() method, since this may not actually unlock the lock when it
        was acquired multiple times recursively. Instead, an internal interface
        of the RLock class is used, which really unlocks it even when it has
        been recursively acquired several times. Another internal interface is
        then used to restore the recursion level when the lock is reacquired.

        """

    def wait_for(self, predicate: Callable[[], _T], timeout: float | None = None) -> _T:
        """Wait until a condition evaluates to True.

        predicate should be a callable which result will be interpreted as a
        boolean value.  A timeout may be provided giving the maximum time to
        wait.

        """

    def notify(self, n: int = 1) -> None:
        """Wake up one or more threads waiting on this condition, if any.

        If the calling thread has not acquired the lock when this method is
        called, a RuntimeError is raised.

        This method wakes up at most n of the threads waiting for the condition
        variable; it is a no-op if no threads are waiting.

        """

    def notify_all(self) -> None:
        """Wake up all threads waiting on this condition.

        If the calling thread has not acquired the lock when this method
        is called, a RuntimeError is raised.

        """

    @deprecated("Deprecated since Python 3.10. Use `notify_all()` instead.")
    def notifyAll(self) -> None:
        """Wake up all threads waiting on this condition.

        This method is deprecated, use notify_all() instead.

        """

class Semaphore:
    """This class implements semaphore objects.

    Semaphores manage a counter representing the number of release() calls minus
    the number of acquire() calls, plus an initial value. The acquire() method
    blocks if necessary until it can return without making the counter
    negative. If not given, value defaults to 1.

    """

    _value: int
    def __init__(self, value: int = 1) -> None: ...
    def __exit__(self, t: type[BaseException] | None, v: BaseException | None, tb: TracebackType | None) -> None: ...
    def acquire(self, blocking: bool = True, timeout: float | None = None) -> bool:
        """Acquire a semaphore, decrementing the internal counter by one.

        When invoked without arguments: if the internal counter is larger than
        zero on entry, decrement it by one and return immediately. If it is zero
        on entry, block, waiting until some other thread has called release() to
        make it larger than zero. This is done with proper interlocking so that
        if multiple acquire() calls are blocked, release() will wake exactly one
        of them up. The implementation may pick one at random, so the order in
        which blocked threads are awakened should not be relied on. There is no
        return value in this case.

        When invoked with blocking set to true, do the same thing as when called
        without arguments, and return true.

        When invoked with blocking set to false, do not block. If a call without
        an argument would block, return false immediately; otherwise, do the
        same thing as when called without arguments, and return true.

        When invoked with a timeout other than None, it will block for at
        most timeout seconds.  If acquire does not complete successfully in
        that interval, return false.  Return true otherwise.

        """

    def __enter__(self, blocking: bool = True, timeout: float | None = None) -> bool:
        """Acquire a semaphore, decrementing the internal counter by one.

        When invoked without arguments: if the internal counter is larger than
        zero on entry, decrement it by one and return immediately. If it is zero
        on entry, block, waiting until some other thread has called release() to
        make it larger than zero. This is done with proper interlocking so that
        if multiple acquire() calls are blocked, release() will wake exactly one
        of them up. The implementation may pick one at random, so the order in
        which blocked threads are awakened should not be relied on. There is no
        return value in this case.

        When invoked with blocking set to true, do the same thing as when called
        without arguments, and return true.

        When invoked with blocking set to false, do not block. If a call without
        an argument would block, return false immediately; otherwise, do the
        same thing as when called without arguments, and return true.

        When invoked with a timeout other than None, it will block for at
        most timeout seconds.  If acquire does not complete successfully in
        that interval, return false.  Return true otherwise.

        """

    def release(self, n: int = 1) -> None:
        """Release a semaphore, incrementing the internal counter by one or more.

        When the counter is zero on entry and another thread is waiting for it
        to become larger than zero again, wake up that thread.

        """

class BoundedSemaphore(Semaphore):
    """Implements a bounded semaphore.

    A bounded semaphore checks to make sure its current value doesn't exceed its
    initial value. If it does, ValueError is raised. In most situations
    semaphores are used to guard resources with limited capacity.

    If the semaphore is released too many times it's a sign of a bug. If not
    given, value defaults to 1.

    Like regular semaphores, bounded semaphores manage a counter representing
    the number of release() calls minus the number of acquire() calls, plus an
    initial value. The acquire() method blocks if necessary until it can return
    without making the counter negative. If not given, value defaults to 1.

    """

class Event:
    """Class implementing event objects.

    Events manage a flag that can be set to true with the set() method and reset
    to false with the clear() method. The wait() method blocks until the flag is
    true.  The flag is initially false.

    """

    def is_set(self) -> bool:
        """Return true if and only if the internal flag is true."""

    @deprecated("Deprecated since Python 3.10. Use `is_set()` instead.")
    def isSet(self) -> bool:
        """Return true if and only if the internal flag is true.

        This method is deprecated, use is_set() instead.

        """

    def set(self) -> None:
        """Set the internal flag to true.

        All threads waiting for it to become true are awakened. Threads
        that call wait() once the flag is true will not block at all.

        """

    def clear(self) -> None:
        """Reset the internal flag to false.

        Subsequently, threads calling wait() will block until set() is called to
        set the internal flag to true again.

        """

    def wait(self, timeout: float | None = None) -> bool:
        """Block until the internal flag is true.

        If the internal flag is true on entry, return immediately. Otherwise,
        block until another thread calls set() to set the flag to true, or until
        the optional timeout occurs.

        When the timeout argument is present and not None, it should be a
        floating-point number specifying a timeout for the operation in seconds
        (or fractions thereof).

        This method returns the internal flag on exit, so it will always return
        ``True`` except if a timeout is given and the operation times out, when
        it will return ``False``.

        """

excepthook: Callable[[_ExceptHookArgs], object]
if sys.version_info >= (3, 10):
    __excepthook__: Callable[[_ExceptHookArgs], object]
ExceptHookArgs = _ExceptHookArgs

class Timer(Thread):
    """Call a function after a specified number of seconds:

    t = Timer(30.0, f, args=None, kwargs=None)
    t.start()
    t.cancel()     # stop the timer's action if it's still waiting

    """

    args: Iterable[Any]  # undocumented
    finished: Event  # undocumented
    function: Callable[..., Any]  # undocumented
    interval: float  # undocumented
    kwargs: Mapping[str, Any]  # undocumented

    def __init__(
        self,
        interval: float,
        function: Callable[..., object],
        args: Iterable[Any] | None = None,
        kwargs: Mapping[str, Any] | None = None,
    ) -> None: ...
    def cancel(self) -> None:
        """Stop the timer if it hasn't finished yet."""

class Barrier:
    """Implements a Barrier.

    Useful for synchronizing a fixed number of threads at known synchronization
    points.  Threads block on 'wait()' and are simultaneously awoken once they
    have all made that call.

    """

    @property
    def parties(self) -> int:
        """Return the number of threads required to trip the barrier."""

    @property
    def n_waiting(self) -> int:
        """Return the number of threads currently waiting at the barrier."""

    @property
    def broken(self) -> bool:
        """Return True if the barrier is in a broken state."""

    def __init__(self, parties: int, action: Callable[[], None] | None = None, timeout: float | None = None) -> None:
        """Create a barrier, initialised to 'parties' threads.

        'action' is a callable which, when supplied, will be called by one of
        the threads after they have all entered the barrier and just prior to
        releasing them all. If a 'timeout' is provided, it is used as the
        default for all subsequent 'wait()' calls.

        """

    def wait(self, timeout: float | None = None) -> int:
        """Wait for the barrier.

        When the specified number of threads have started waiting, they are all
        simultaneously awoken. If an 'action' was provided for the barrier, one
        of the threads will have executed that callback prior to returning.
        Returns an individual index number from 0 to 'parties-1'.

        """

    def reset(self) -> None:
        """Reset the barrier to the initial state.

        Any threads currently waiting will get the BrokenBarrier exception
        raised.

        """

    def abort(self) -> None:
        """Place the barrier into a 'broken' state.

        Useful in case of error.  Any currently waiting threads and threads
        attempting to 'wait()' will have BrokenBarrierError raised.

        """

class BrokenBarrierError(RuntimeError): ...

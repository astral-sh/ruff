"""This module provides primitive operations to write multi-threaded programs.
The 'threading' module provides a more convenient interface.
"""

import signal
import sys
from _typeshed import structseq
from collections.abc import Callable
from threading import Thread
from types import TracebackType
from typing import Any, Final, NoReturn, final, overload
from typing_extensions import TypeVarTuple, Unpack, disjoint_base

_Ts = TypeVarTuple("_Ts")

error = RuntimeError

def _count() -> int:
    """Return the number of currently running Python threads, excluding
    the main thread. The returned number comprises all threads created
    through `start_new_thread()` as well as `threading.Thread`, and not
    yet finished.

    This function is meant for internal and specialized purposes only.
    In most applications `threading.enumerate()` should be used instead.
    """

@final
class RLock:
    def acquire(self, blocking: bool = True, timeout: float = -1) -> bool:
        """Lock the lock.  `blocking` indicates whether we should wait
        for the lock to be available or not.  If `blocking` is False
        and another thread holds the lock, the method will return False
        immediately.  If `blocking` is True and another thread holds
        the lock, the method will wait for the lock to be released,
        take it and then return True.
        (note: the blocking operation is interruptible.)

        In all other cases, the method will return True immediately.
        Precisely, if the current thread already holds the lock, its
        internal counter is simply incremented. If nobody holds the lock,
        the lock is taken and its internal counter initialized to 1.
        """

    def release(self) -> None:
        """Release the lock, allowing another thread that is blocked waiting for
        the lock to acquire the lock.  The lock must be in the locked state,
        and must be locked by the same thread that unlocks it; otherwise a
        `RuntimeError` is raised.

        Do note that if the lock was acquire()d several times in a row by the
        current thread, release() needs to be called as many times for the lock
        to be available for other threads.
        """
    __enter__ = acquire
    def __exit__(self, t: type[BaseException] | None, v: BaseException | None, tb: TracebackType | None) -> None:
        """Release the lock."""
    if sys.version_info >= (3, 14):
        def locked(self) -> bool:
            """locked()

            Return a boolean indicating whether this object is locked right now.
            """

if sys.version_info >= (3, 13):
    @final
    class _ThreadHandle:
        ident: int

        def join(self, timeout: float | None = None, /) -> None: ...
        def is_done(self) -> bool: ...
        def _set_done(self) -> None: ...

    def start_joinable_thread(
        function: Callable[[], object], handle: _ThreadHandle | None = None, daemon: bool = True
    ) -> _ThreadHandle:
        """*For internal use only*: start a new thread.

        Like start_new_thread(), this starts a new thread calling the given function.
        Unlike start_new_thread(), this returns a handle object with methods to join
        or detach the given thread.
        This function is not for third-party code, please use the
        `threading` module instead. During finalization the runtime will not wait for
        the thread to exit if daemon is True. If handle is provided it must be a
        newly created thread._ThreadHandle instance.
        """

    @final
    class lock:
        """A lock object is a synchronization primitive.  To create a lock,
        call threading.Lock().  Methods are:

        acquire() -- lock the lock, possibly blocking until it can be obtained
        release() -- unlock of the lock
        locked() -- test whether the lock is currently locked

        A lock is not owned by the thread that locked it; another thread may
        unlock it.  A thread attempting to lock a lock that it has already locked
        will block until another thread unlocks it.  Deadlocks may ensue.
        """

        def acquire(self, blocking: bool = True, timeout: float = -1) -> bool:
            """Lock the lock.  Without argument, this blocks if the lock is already
            locked (even by the same thread), waiting for another thread to release
            the lock, and return True once the lock is acquired.
            With an argument, this will only block if the argument is true,
            and the return value reflects whether the lock is acquired.
            The blocking operation is interruptible.
            """

        def release(self) -> None:
            """Release the lock, allowing another thread that is blocked waiting for
            the lock to acquire the lock.  The lock must be in the locked state,
            but it needn't be locked by the same thread that unlocks it.
            """

        def locked(self) -> bool:
            """Return whether the lock is in the locked state."""

        def acquire_lock(self, blocking: bool = True, timeout: float = -1) -> bool:
            """An obsolete synonym of acquire()."""

        def release_lock(self) -> None:
            """An obsolete synonym of release()."""

        def locked_lock(self) -> bool:
            """An obsolete synonym of locked()."""

        def __enter__(self) -> bool:
            """Lock the lock."""

        def __exit__(
            self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
        ) -> None:
            """Release the lock."""

    LockType = lock
else:
    @final
    class LockType:
        """A lock object is a synchronization primitive.  To create a lock,
        call threading.Lock().  Methods are:

        acquire() -- lock the lock, possibly blocking until it can be obtained
        release() -- unlock of the lock
        locked() -- test whether the lock is currently locked

        A lock is not owned by the thread that locked it; another thread may
        unlock it.  A thread attempting to lock a lock that it has already locked
        will block until another thread unlocks it.  Deadlocks may ensue.
        """

        def acquire(self, blocking: bool = True, timeout: float = -1) -> bool:
            """acquire(blocking=True, timeout=-1) -> bool
            (acquire_lock() is an obsolete synonym)

            Lock the lock.  Without argument, this blocks if the lock is already
            locked (even by the same thread), waiting for another thread to release
            the lock, and return True once the lock is acquired.
            With an argument, this will only block if the argument is true,
            and the return value reflects whether the lock is acquired.
            The blocking operation is interruptible.
            """

        def release(self) -> None:
            """release()
            (release_lock() is an obsolete synonym)

            Release the lock, allowing another thread that is blocked waiting for
            the lock to acquire the lock.  The lock must be in the locked state,
            but it needn't be locked by the same thread that unlocks it.
            """

        def locked(self) -> bool:
            """locked() -> bool
            (locked_lock() is an obsolete synonym)

            Return whether the lock is in the locked state.
            """

        def acquire_lock(self, blocking: bool = True, timeout: float = -1) -> bool:
            """acquire(blocking=True, timeout=-1) -> bool
            (acquire_lock() is an obsolete synonym)

            Lock the lock.  Without argument, this blocks if the lock is already
            locked (even by the same thread), waiting for another thread to release
            the lock, and return True once the lock is acquired.
            With an argument, this will only block if the argument is true,
            and the return value reflects whether the lock is acquired.
            The blocking operation is interruptible.
            """

        def release_lock(self) -> None:
            """release()
            (release_lock() is an obsolete synonym)

            Release the lock, allowing another thread that is blocked waiting for
            the lock to acquire the lock.  The lock must be in the locked state,
            but it needn't be locked by the same thread that unlocks it.
            """

        def locked_lock(self) -> bool:
            """locked() -> bool
            (locked_lock() is an obsolete synonym)

            Return whether the lock is in the locked state.
            """

        def __enter__(self) -> bool:
            """acquire(blocking=True, timeout=-1) -> bool
            (acquire_lock() is an obsolete synonym)

            Lock the lock.  Without argument, this blocks if the lock is already
            locked (even by the same thread), waiting for another thread to release
            the lock, and return True once the lock is acquired.
            With an argument, this will only block if the argument is true,
            and the return value reflects whether the lock is acquired.
            The blocking operation is interruptible.
            """

        def __exit__(
            self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
        ) -> None:
            """release()
            (release_lock() is an obsolete synonym)

            Release the lock, allowing another thread that is blocked waiting for
            the lock to acquire the lock.  The lock must be in the locked state,
            but it needn't be locked by the same thread that unlocks it.
            """

@overload
def start_new_thread(function: Callable[[Unpack[_Ts]], object], args: tuple[Unpack[_Ts]], /) -> int:
    """Start a new thread and return its identifier.

    The thread will call the function with positional arguments from the
    tuple args and keyword arguments taken from the optional dictionary
    kwargs.  The thread exits when the function returns; the return value
    is ignored.  The thread will also exit when the function raises an
    unhandled exception; a stack trace will be printed unless the exception
    is SystemExit.
    """

@overload
def start_new_thread(function: Callable[..., object], args: tuple[Any, ...], kwargs: dict[str, Any], /) -> int: ...

# Obsolete synonym for start_new_thread()
@overload
def start_new(function: Callable[[Unpack[_Ts]], object], args: tuple[Unpack[_Ts]], /) -> int:
    """An obsolete synonym of start_new_thread()."""

@overload
def start_new(function: Callable[..., object], args: tuple[Any, ...], kwargs: dict[str, Any], /) -> int: ...

if sys.version_info >= (3, 10):
    def interrupt_main(signum: signal.Signals = signal.SIGINT, /) -> None:
        """Simulate the arrival of the given signal in the main thread,
        where the corresponding signal handler will be executed.
        If *signum* is omitted, SIGINT is assumed.
        A subthread can use this function to interrupt the main thread.

        Note: the default signal handler for SIGINT raises ``KeyboardInterrupt``.
        """

else:
    def interrupt_main() -> None:
        """interrupt_main()

        Raise a KeyboardInterrupt in the main thread.
        A subthread can use this function to interrupt the main thread.
        """

def exit() -> NoReturn:
    """This is synonymous to ``raise SystemExit''.  It will cause the current
    thread to exit silently unless the exception is caught.
    """

def exit_thread() -> NoReturn:  # Obsolete synonym for exit()
    """An obsolete synonym of exit()."""

def allocate_lock() -> LockType:
    """Create a new lock object. See help(type(threading.Lock())) for
    information about locks.
    """

def allocate() -> LockType:  # Obsolete synonym for allocate_lock()
    """An obsolete synonym of allocate_lock()."""

def get_ident() -> int:
    """Return a non-zero integer that uniquely identifies the current thread
    amongst other threads that exist simultaneously.
    This may be used to identify per-thread resources.
    Even though on some platforms threads identities may appear to be
    allocated consecutive numbers starting at 1, this behavior should not
    be relied upon, and the number should be seen purely as a magic cookie.
    A thread's identity may be reused for another thread after it exits.
    """

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

def get_native_id() -> int:  # only available on some platforms
    """Return a non-negative integer identifying the thread as reported
    by the OS (kernel). This may be used to uniquely identify a
    particular thread within a system.
    """

@final
class _ExceptHookArgs(structseq[Any], tuple[type[BaseException], BaseException | None, TracebackType | None, Thread | None]):
    """ExceptHookArgs

    Type used to pass arguments to threading.excepthook.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = ("exc_type", "exc_value", "exc_traceback", "thread")

    @property
    def exc_type(self) -> type[BaseException]:
        """Exception type"""

    @property
    def exc_value(self) -> BaseException | None:
        """Exception value"""

    @property
    def exc_traceback(self) -> TracebackType | None:
        """Exception traceback"""

    @property
    def thread(self) -> Thread | None:
        """Thread"""

_excepthook: Callable[[_ExceptHookArgs], Any]

if sys.version_info >= (3, 12):
    def daemon_threads_allowed() -> bool:
        """Return True if daemon threads are allowed in the current interpreter,
        and False otherwise.
        """

if sys.version_info >= (3, 14):
    def set_name(name: str) -> None:
        """Set the name of the current thread."""

@disjoint_base
class _local:
    """Thread-local data"""

    def __getattribute__(self, name: str, /) -> Any: ...
    def __setattr__(self, name: str, value: Any, /) -> None: ...
    def __delattr__(self, name: str, /) -> None: ...

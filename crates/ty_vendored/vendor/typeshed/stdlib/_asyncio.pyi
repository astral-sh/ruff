"""Accelerator module for asyncio"""

import sys
from asyncio.events import AbstractEventLoop
from collections.abc import Awaitable, Callable, Coroutine, Generator
from contextvars import Context
from types import FrameType, GenericAlias
from typing import Any, Literal, TextIO, TypeVar
from typing_extensions import Self, TypeAlias, disjoint_base

_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)
_TaskYieldType: TypeAlias = Future[object] | None

@disjoint_base
class Future(Awaitable[_T]):
    """This class is *almost* compatible with concurrent.futures.Future.

    Differences:

    - result() and exception() do not take a timeout argument and
      raise an exception when the future isn't done yet.

    - Callbacks registered with add_done_callback() are always called
      via the event loop's call_soon_threadsafe().

    - This class is not compatible with the wait() and as_completed()
      methods in the concurrent.futures package.
    """

    _state: str
    @property
    def _exception(self) -> BaseException | None: ...
    _blocking: bool
    @property
    def _log_traceback(self) -> bool: ...
    @_log_traceback.setter
    def _log_traceback(self, val: Literal[False]) -> None: ...
    _asyncio_future_blocking: bool  # is a part of duck-typing contract for `Future`
    def __init__(self, *, loop: AbstractEventLoop | None = None) -> None: ...
    def __del__(self) -> None:
        """Called when the instance is about to be destroyed."""

    def get_loop(self) -> AbstractEventLoop:
        """Return the event loop the Future is bound to."""

    @property
    def _callbacks(self) -> list[tuple[Callable[[Self], Any], Context]]: ...
    def add_done_callback(self, fn: Callable[[Self], object], /, *, context: Context | None = None) -> None:
        """Add a callback to be run when the future becomes done.

        The callback is called with a single argument - the future object. If
        the future is already done when this is called, the callback is
        scheduled with call_soon.
        """

    def cancel(self, msg: Any | None = None) -> bool:
        """Cancel the future and schedule callbacks.

        If the future is already done or cancelled, return False.  Otherwise,
        change the future's state to cancelled, schedule the callbacks and
        return True.
        """

    def cancelled(self) -> bool:
        """Return True if the future was cancelled."""

    def done(self) -> bool:
        """Return True if the future is done.

        Done means either that a result / exception are available, or that the
        future was cancelled.
        """

    def result(self) -> _T:
        """Return the result this future represents.

        If the future has been cancelled, raises CancelledError.  If the
        future's result isn't yet available, raises InvalidStateError.  If
        the future is done and has an exception set, this exception is raised.
        """

    def exception(self) -> BaseException | None:
        """Return the exception that was set on this future.

        The exception (or None if no exception was set) is returned only if
        the future is done.  If the future has been cancelled, raises
        CancelledError.  If the future isn't done yet, raises
        InvalidStateError.
        """

    def remove_done_callback(self, fn: Callable[[Self], object], /) -> int:
        """Remove all instances of a callback from the "call when done" list.

        Returns the number of callbacks removed.
        """

    def set_result(self, result: _T, /) -> None:
        """Mark the future done and set its result.

        If the future is already done when this method is called, raises
        InvalidStateError.
        """

    def set_exception(self, exception: type | BaseException, /) -> None:
        """Mark the future done and set an exception.

        If the future is already done when this method is called, raises
        InvalidStateError.
        """

    def __iter__(self) -> Generator[Any, None, _T]:
        """Implement iter(self)."""

    def __await__(self) -> Generator[Any, None, _T]:
        """Return an iterator to be used in await expression."""

    @property
    def _loop(self) -> AbstractEventLoop: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

if sys.version_info >= (3, 12):
    _TaskCompatibleCoro: TypeAlias = Coroutine[Any, Any, _T_co]
else:
    _TaskCompatibleCoro: TypeAlias = Generator[_TaskYieldType, None, _T_co] | Coroutine[Any, Any, _T_co]

# mypy and pyright complain that a subclass of an invariant class shouldn't be covariant.
# While this is true in general, here it's sort-of okay to have a covariant subclass,
# since the only reason why `asyncio.Future` is invariant is the `set_result()` method,
# and `asyncio.Task.set_result()` always raises.
@disjoint_base
class Task(Future[_T_co]):  # type: ignore[type-var]  # pyright: ignore[reportInvalidTypeArguments]
    """A coroutine wrapped in a Future."""

    if sys.version_info >= (3, 12):
        def __init__(
            self,
            coro: _TaskCompatibleCoro[_T_co],
            *,
            loop: AbstractEventLoop | None = None,
            name: str | None = None,
            context: Context | None = None,
            eager_start: bool = False,
        ) -> None: ...
    elif sys.version_info >= (3, 11):
        def __init__(
            self,
            coro: _TaskCompatibleCoro[_T_co],
            *,
            loop: AbstractEventLoop | None = None,
            name: str | None = None,
            context: Context | None = None,
        ) -> None: ...
    else:
        def __init__(
            self, coro: _TaskCompatibleCoro[_T_co], *, loop: AbstractEventLoop | None = None, name: str | None = None
        ) -> None: ...

    if sys.version_info >= (3, 12):
        def get_coro(self) -> _TaskCompatibleCoro[_T_co] | None: ...
    else:
        def get_coro(self) -> _TaskCompatibleCoro[_T_co]: ...

    def get_name(self) -> str: ...
    def set_name(self, value: object, /) -> None: ...
    if sys.version_info >= (3, 12):
        def get_context(self) -> Context: ...

    def get_stack(self, *, limit: int | None = None) -> list[FrameType]:
        """Return the list of stack frames for this task's coroutine.

        If the coroutine is not done, this returns the stack where it is
        suspended.  If the coroutine has completed successfully or was
        cancelled, this returns an empty list.  If the coroutine was
        terminated by an exception, this returns the list of traceback
        frames.

        The frames are always ordered from oldest to newest.

        The optional limit gives the maximum number of frames to
        return; by default all available frames are returned.  Its
        meaning differs depending on whether a stack or a traceback is
        returned: the newest frames of a stack are returned, but the
        oldest frames of a traceback are returned.  (This matches the
        behavior of the traceback module.)

        For reasons beyond our control, only one stack frame is
        returned for a suspended coroutine.
        """

    def print_stack(self, *, limit: int | None = None, file: TextIO | None = None) -> None:
        """Print the stack or traceback for this task's coroutine.

        This produces output similar to that of the traceback module,
        for the frames retrieved by get_stack().  The limit argument
        is passed to get_stack().  The file argument is an I/O stream
        to which the output is written; by default output is written
        to sys.stderr.
        """
    if sys.version_info >= (3, 11):
        def cancelling(self) -> int:
            """Return the count of the task's cancellation requests.

            This count is incremented when .cancel() is called
            and may be decremented using .uncancel().
            """

        def uncancel(self) -> int:
            """Decrement the task's count of cancellation requests.

            This should be used by tasks that catch CancelledError
            and wish to continue indefinitely until they are cancelled again.

            Returns the remaining number of cancellation requests.
            """

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

def get_event_loop() -> AbstractEventLoop:
    """Return an asyncio event loop.

    When called from a coroutine or a callback (e.g. scheduled with
    call_soon or similar API), this function will always return the
    running event loop.

    If there is no running event loop set, the function will return
    the result of `get_event_loop_policy().get_event_loop()` call.
    """

def get_running_loop() -> AbstractEventLoop:
    """Return the running event loop.  Raise a RuntimeError if there is none.

    This function is thread-specific.
    """

def _set_running_loop(loop: AbstractEventLoop | None, /) -> None:
    """Set the running event loop.

    This is a low-level function intended to be used by event loops.
    This function is thread-specific.
    """

def _get_running_loop() -> AbstractEventLoop:
    """Return the running event loop or None.

    This is a low-level function intended to be used by event loops.
    This function is thread-specific.
    """

def _register_task(task: Task[Any]) -> None:
    """Register a new task in asyncio as executed by loop.

    Returns None.
    """

def _unregister_task(task: Task[Any]) -> None:
    """Unregister a task.

    Returns None.
    """

def _enter_task(loop: AbstractEventLoop, task: Task[Any]) -> None:
    """Enter into task execution or resume suspended task.

    Task belongs to loop.

    Returns None.
    """

def _leave_task(loop: AbstractEventLoop, task: Task[Any]) -> None:
    """Leave task execution or suspend a task.

    Task belongs to loop.

    Returns None.
    """

if sys.version_info >= (3, 12):
    def current_task(loop: AbstractEventLoop | None = None) -> Task[Any] | None:
        """Return a currently executed task."""

if sys.version_info >= (3, 14):
    def future_discard_from_awaited_by(future: Future[Any], waiter: Future[Any], /) -> None: ...
    def future_add_to_awaited_by(future: Future[Any], waiter: Future[Any], /) -> None:
        """Record that `fut` is awaited on by `waiter`."""

    def all_tasks(loop: AbstractEventLoop | None = None) -> set[Task[Any]]:
        """Return a set of all tasks for the loop."""

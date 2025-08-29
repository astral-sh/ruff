import sys
import threading
from _typeshed import Unused
from collections.abc import Callable, Iterable, Iterator
from logging import Logger
from types import GenericAlias, TracebackType
from typing import Any, Final, Generic, NamedTuple, Protocol, TypeVar, type_check_only
from typing_extensions import ParamSpec, Self

FIRST_COMPLETED: Final = "FIRST_COMPLETED"
FIRST_EXCEPTION: Final = "FIRST_EXCEPTION"
ALL_COMPLETED: Final = "ALL_COMPLETED"
PENDING: Final = "PENDING"
RUNNING: Final = "RUNNING"
CANCELLED: Final = "CANCELLED"
CANCELLED_AND_NOTIFIED: Final = "CANCELLED_AND_NOTIFIED"
FINISHED: Final = "FINISHED"
_STATE_TO_DESCRIPTION_MAP: Final[dict[str, str]]
LOGGER: Logger

class Error(Exception):
    """Base class for all future-related exceptions."""

class CancelledError(Error):
    """The Future was cancelled."""

if sys.version_info >= (3, 11):
    from builtins import TimeoutError as TimeoutError
else:
    class TimeoutError(Error):
        """The operation exceeded the given deadline."""

class InvalidStateError(Error):
    """The operation is not allowed in this state."""

class BrokenExecutor(RuntimeError):
    """
    Raised when a executor has become non-functional after a severe failure.
    """

_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)
_P = ParamSpec("_P")

class Future(Generic[_T]):
    """Represents the result of an asynchronous computation."""

    _condition: threading.Condition
    _state: str
    _result: _T | None
    _exception: BaseException | None
    _waiters: list[_Waiter]
    def cancel(self) -> bool:
        """Cancel the future if possible.

        Returns True if the future was cancelled, False otherwise. A future
        cannot be cancelled if it is running or has already completed.
        """

    def cancelled(self) -> bool:
        """Return True if the future was cancelled."""

    def running(self) -> bool:
        """Return True if the future is currently executing."""

    def done(self) -> bool:
        """Return True if the future was cancelled or finished executing."""

    def add_done_callback(self, fn: Callable[[Future[_T]], object]) -> None:
        """Attaches a callable that will be called when the future finishes.

        Args:
            fn: A callable that will be called with this future as its only
                argument when the future completes or is cancelled. The callable
                will always be called by a thread in the same process in which
                it was added. If the future has already completed or been
                cancelled then the callable will be called immediately. These
                callables are called in the order that they were added.
        """

    def result(self, timeout: float | None = None) -> _T:
        """Return the result of the call that the future represents.

        Args:
            timeout: The number of seconds to wait for the result if the future
                isn't done. If None, then there is no limit on the wait time.

        Returns:
            The result of the call that the future represents.

        Raises:
            CancelledError: If the future was cancelled.
            TimeoutError: If the future didn't finish executing before the given
                timeout.
            Exception: If the call raised then that exception will be raised.
        """

    def set_running_or_notify_cancel(self) -> bool:
        """Mark the future as running or process any cancel notifications.

        Should only be used by Executor implementations and unit tests.

        If the future has been cancelled (cancel() was called and returned
        True) then any threads waiting on the future completing (though calls
        to as_completed() or wait()) are notified and False is returned.

        If the future was not cancelled then it is put in the running state
        (future calls to running() will return True) and True is returned.

        This method should be called by Executor implementations before
        executing the work associated with this future. If this method returns
        False then the work should not be executed.

        Returns:
            False if the Future was cancelled, True otherwise.

        Raises:
            RuntimeError: if this method was already called or if set_result()
                or set_exception() was called.
        """

    def set_result(self, result: _T) -> None:
        """Sets the return value of work associated with the future.

        Should only be used by Executor implementations and unit tests.
        """

    def exception(self, timeout: float | None = None) -> BaseException | None:
        """Return the exception raised by the call that the future represents.

        Args:
            timeout: The number of seconds to wait for the exception if the
                future isn't done. If None, then there is no limit on the wait
                time.

        Returns:
            The exception raised by the call that the future represents or None
            if the call completed without raising.

        Raises:
            CancelledError: If the future was cancelled.
            TimeoutError: If the future didn't finish executing before the given
                timeout.
        """

    def set_exception(self, exception: BaseException | None) -> None:
        """Sets the result of the future as being the given exception.

        Should only be used by Executor implementations and unit tests.
        """

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

class Executor:
    """This is an abstract base class for concrete asynchronous executors."""

    def submit(self, fn: Callable[_P, _T], /, *args: _P.args, **kwargs: _P.kwargs) -> Future[_T]:
        """Submits a callable to be executed with the given arguments.

        Schedules the callable to be executed as fn(*args, **kwargs) and returns
        a Future instance representing the execution of the callable.

        Returns:
            A Future representing the given call.
        """
    if sys.version_info >= (3, 14):
        def map(
            self,
            fn: Callable[..., _T],
            *iterables: Iterable[Any],
            timeout: float | None = None,
            chunksize: int = 1,
            buffersize: int | None = None,
        ) -> Iterator[_T]:
            """Returns an iterator equivalent to map(fn, iter).

            Args:
                fn: A callable that will take as many arguments as there are
                    passed iterables.
                timeout: The maximum number of seconds to wait. If None, then there
                    is no limit on the wait time.
                chunksize: The size of the chunks the iterable will be broken into
                    before being passed to a child process. This argument is only
                    used by ProcessPoolExecutor; it is ignored by
                    ThreadPoolExecutor.
                buffersize: The number of submitted tasks whose results have not
                    yet been yielded. If the buffer is full, iteration over the
                    iterables pauses until a result is yielded from the buffer.
                    If None, all input elements are eagerly collected, and a task is
                    submitted for each.

            Returns:
                An iterator equivalent to: map(func, *iterables) but the calls may
                be evaluated out-of-order.

            Raises:
                TimeoutError: If the entire result iterator could not be generated
                    before the given timeout.
                Exception: If fn(*args) raises for any values.
            """
    else:
        def map(
            self, fn: Callable[..., _T], *iterables: Iterable[Any], timeout: float | None = None, chunksize: int = 1
        ) -> Iterator[_T]:
            """Returns an iterator equivalent to map(fn, iter).

            Args:
                fn: A callable that will take as many arguments as there are
                    passed iterables.
                timeout: The maximum number of seconds to wait. If None, then there
                    is no limit on the wait time.
                chunksize: The size of the chunks the iterable will be broken into
                    before being passed to a child process. This argument is only
                    used by ProcessPoolExecutor; it is ignored by
                    ThreadPoolExecutor.

            Returns:
                An iterator equivalent to: map(func, *iterables) but the calls may
                be evaluated out-of-order.

            Raises:
                TimeoutError: If the entire result iterator could not be generated
                    before the given timeout.
                Exception: If fn(*args) raises for any values.
            """

    def shutdown(self, wait: bool = True, *, cancel_futures: bool = False) -> None:
        """Clean-up the resources associated with the Executor.

        It is safe to call this method several times. Otherwise, no other
        methods can be called after this one.

        Args:
            wait: If True then shutdown will not return until all running
                futures have finished executing and the resources used by the
                executor have been reclaimed.
            cancel_futures: If True then shutdown will cancel all pending
                futures. Futures that are completed or running will not be
                cancelled.
        """

    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> bool | None: ...

@type_check_only
class _AsCompletedFuture(Protocol[_T_co]):
    # as_completed only mutates non-generic aspects of passed Futures and does not do any nominal
    # checks. Therefore, we can use a Protocol here to allow as_completed to act covariantly.
    # See the tests for concurrent.futures
    _condition: threading.Condition
    _state: str
    _waiters: list[_Waiter]
    # Not used by as_completed, but needed to propagate the generic type
    def result(self, timeout: float | None = None) -> _T_co: ...

def as_completed(fs: Iterable[_AsCompletedFuture[_T]], timeout: float | None = None) -> Iterator[Future[_T]]:
    """An iterator over the given futures that yields each as it completes.

    Args:
        fs: The sequence of Futures (possibly created by different Executors) to
            iterate over.
        timeout: The maximum number of seconds to wait. If None, then there
            is no limit on the wait time.

    Returns:
        An iterator that yields the given Futures as they complete (finished or
        cancelled). If any given Futures are duplicated, they will be returned
        once.

    Raises:
        TimeoutError: If the entire result iterator could not be generated
            before the given timeout.
    """

class DoneAndNotDoneFutures(NamedTuple, Generic[_T]):
    """DoneAndNotDoneFutures(done, not_done)"""

    done: set[Future[_T]]
    not_done: set[Future[_T]]

def wait(fs: Iterable[Future[_T]], timeout: float | None = None, return_when: str = "ALL_COMPLETED") -> DoneAndNotDoneFutures[_T]:
    """Wait for the futures in the given sequence to complete.

    Args:
        fs: The sequence of Futures (possibly created by different Executors) to
            wait upon.
        timeout: The maximum number of seconds to wait. If None, then there
            is no limit on the wait time.
        return_when: Indicates when this function should return. The options
            are:

            FIRST_COMPLETED - Return when any future finishes or is
                              cancelled.
            FIRST_EXCEPTION - Return when any future finishes by raising an
                              exception. If no future raises an exception
                              then it is equivalent to ALL_COMPLETED.
            ALL_COMPLETED -   Return when all futures finish or are cancelled.

    Returns:
        A named 2-tuple of sets. The first set, named 'done', contains the
        futures that completed (is finished or cancelled) before the wait
        completed. The second set, named 'not_done', contains uncompleted
        futures. Duplicate futures given to *fs* are removed and will be
        returned only once.
    """

class _Waiter:
    """Provides the event that wait() and as_completed() block on."""

    event: threading.Event
    finished_futures: list[Future[Any]]
    def add_result(self, future: Future[Any]) -> None: ...
    def add_exception(self, future: Future[Any]) -> None: ...
    def add_cancelled(self, future: Future[Any]) -> None: ...

class _AsCompletedWaiter(_Waiter):
    """Used by as_completed()."""

    lock: threading.Lock

class _FirstCompletedWaiter(_Waiter):
    """Used by wait(return_when=FIRST_COMPLETED)."""

class _AllCompletedWaiter(_Waiter):
    """Used by wait(return_when=FIRST_EXCEPTION and ALL_COMPLETED)."""

    num_pending_calls: int
    stop_on_exception: bool
    lock: threading.Lock
    def __init__(self, num_pending_calls: int, stop_on_exception: bool) -> None: ...

class _AcquireFutures:
    """A context manager that does an ordered acquire of Future conditions."""

    futures: Iterable[Future[Any]]
    def __init__(self, futures: Iterable[Future[Any]]) -> None: ...
    def __enter__(self) -> None: ...
    def __exit__(self, *args: Unused) -> None: ...

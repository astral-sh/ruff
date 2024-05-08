import sys
import threading
from _typeshed import Unused
from collections.abc import Callable, Collection, Iterable, Iterator
from logging import Logger
from types import TracebackType
from typing import Any, Generic, Literal, NamedTuple, Protocol, TypeVar
from typing_extensions import ParamSpec, Self

if sys.version_info >= (3, 9):
    from types import GenericAlias

FIRST_COMPLETED: Literal["FIRST_COMPLETED"]
FIRST_EXCEPTION: Literal["FIRST_EXCEPTION"]
ALL_COMPLETED: Literal["ALL_COMPLETED"]
PENDING: Literal["PENDING"]
RUNNING: Literal["RUNNING"]
CANCELLED: Literal["CANCELLED"]
CANCELLED_AND_NOTIFIED: Literal["CANCELLED_AND_NOTIFIED"]
FINISHED: Literal["FINISHED"]
_FUTURE_STATES: list[str]
_STATE_TO_DESCRIPTION_MAP: dict[str, str]
LOGGER: Logger

class Error(Exception): ...
class CancelledError(Error): ...

if sys.version_info >= (3, 11):
    from builtins import TimeoutError as TimeoutError
else:
    class TimeoutError(Error): ...

class InvalidStateError(Error): ...
class BrokenExecutor(RuntimeError): ...

_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)
_P = ParamSpec("_P")

class Future(Generic[_T]):
    _condition: threading.Condition
    _state: str
    _result: _T | None
    _exception: BaseException | None
    _waiters: list[_Waiter]
    def cancel(self) -> bool: ...
    def cancelled(self) -> bool: ...
    def running(self) -> bool: ...
    def done(self) -> bool: ...
    def add_done_callback(self, fn: Callable[[Future[_T]], object]) -> None: ...
    def result(self, timeout: float | None = None) -> _T: ...
    def set_running_or_notify_cancel(self) -> bool: ...
    def set_result(self, result: _T) -> None: ...
    def exception(self, timeout: float | None = None) -> BaseException | None: ...
    def set_exception(self, exception: BaseException | None) -> None: ...
    if sys.version_info >= (3, 9):
        def __class_getitem__(cls, item: Any) -> GenericAlias: ...

class Executor:
    if sys.version_info >= (3, 9):
        def submit(self, fn: Callable[_P, _T], /, *args: _P.args, **kwargs: _P.kwargs) -> Future[_T]: ...
    else:
        def submit(self, fn: Callable[_P, _T], *args: _P.args, **kwargs: _P.kwargs) -> Future[_T]: ...

    def map(
        self, fn: Callable[..., _T], *iterables: Iterable[Any], timeout: float | None = None, chunksize: int = 1
    ) -> Iterator[_T]: ...
    if sys.version_info >= (3, 9):
        def shutdown(self, wait: bool = True, *, cancel_futures: bool = False) -> None: ...
    else:
        def shutdown(self, wait: bool = True) -> None: ...

    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> bool | None: ...

class _AsCompletedFuture(Protocol[_T_co]):
    # as_completed only mutates non-generic aspects of passed Futures and does not do any nominal
    # checks. Therefore, we can use a Protocol here to allow as_completed to act covariantly.
    # See the tests for concurrent.futures
    _condition: threading.Condition
    _state: str
    _waiters: list[_Waiter]
    # Not used by as_completed, but needed to propagate the generic type
    def result(self, timeout: float | None = None) -> _T_co: ...

def as_completed(fs: Iterable[_AsCompletedFuture[_T]], timeout: float | None = None) -> Iterator[Future[_T]]: ...

class DoneAndNotDoneFutures(NamedTuple, Generic[_T]):
    done: set[Future[_T]]
    not_done: set[Future[_T]]

if sys.version_info >= (3, 9):
    def wait(
        fs: Iterable[Future[_T]], timeout: float | None = None, return_when: str = "ALL_COMPLETED"
    ) -> DoneAndNotDoneFutures[_T]: ...

else:
    def wait(
        fs: Collection[Future[_T]], timeout: float | None = None, return_when: str = "ALL_COMPLETED"
    ) -> DoneAndNotDoneFutures[_T]: ...

class _Waiter:
    event: threading.Event
    finished_futures: list[Future[Any]]
    def add_result(self, future: Future[Any]) -> None: ...
    def add_exception(self, future: Future[Any]) -> None: ...
    def add_cancelled(self, future: Future[Any]) -> None: ...

class _AsCompletedWaiter(_Waiter):
    lock: threading.Lock

class _FirstCompletedWaiter(_Waiter): ...

class _AllCompletedWaiter(_Waiter):
    num_pending_calls: int
    stop_on_exception: bool
    lock: threading.Lock
    def __init__(self, num_pending_calls: int, stop_on_exception: bool) -> None: ...

class _AcquireFutures:
    futures: Iterable[Future[Any]]
    def __init__(self, futures: Iterable[Future[Any]]) -> None: ...
    def __enter__(self) -> None: ...
    def __exit__(self, *args: Unused) -> None: ...

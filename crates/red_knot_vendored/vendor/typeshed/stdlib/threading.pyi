import _thread
import sys
from _thread import _excepthook, _ExceptHookArgs, get_native_id as get_native_id
from _typeshed import ProfileFunction, TraceFunction
from collections.abc import Callable, Iterable, Mapping
from types import TracebackType
from typing import Any, TypeVar, final

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

def active_count() -> int: ...
def activeCount() -> int: ...  # deprecated alias for active_count()
def current_thread() -> Thread: ...
def currentThread() -> Thread: ...  # deprecated alias for current_thread()
def get_ident() -> int: ...
def enumerate() -> list[Thread]: ...
def main_thread() -> Thread: ...
def settrace(func: TraceFunction) -> None: ...
def setprofile(func: ProfileFunction | None) -> None: ...

if sys.version_info >= (3, 12):
    def setprofile_all_threads(func: ProfileFunction | None) -> None: ...
    def settrace_all_threads(func: TraceFunction) -> None: ...

if sys.version_info >= (3, 10):
    def gettrace() -> TraceFunction | None: ...
    def getprofile() -> ProfileFunction | None: ...

def stack_size(size: int = 0, /) -> int: ...

TIMEOUT_MAX: float

ThreadError = _thread.error
local = _thread._local

class Thread:
    name: str
    @property
    def ident(self) -> int | None: ...
    daemon: bool
    def __init__(
        self,
        group: None = None,
        target: Callable[..., object] | None = None,
        name: str | None = None,
        args: Iterable[Any] = (),
        kwargs: Mapping[str, Any] | None = None,
        *,
        daemon: bool | None = None,
    ) -> None: ...
    def start(self) -> None: ...
    def run(self) -> None: ...
    def join(self, timeout: float | None = None) -> None: ...
    @property
    def native_id(self) -> int | None: ...  # only available on some platforms
    def is_alive(self) -> bool: ...
    # the following methods are all deprecated
    def getName(self) -> str: ...
    def setName(self, name: str) -> None: ...
    def isDaemon(self) -> bool: ...
    def setDaemon(self, daemonic: bool) -> None: ...

class _DummyThread(Thread):
    def __init__(self) -> None: ...

# This is actually the function _thread.allocate_lock for <= 3.12
Lock = _thread.LockType

# Python implementation of RLock.
@final
class _RLock:
    _count: int
    def acquire(self, blocking: bool = True, timeout: float = -1) -> bool: ...
    def release(self) -> None: ...
    __enter__ = acquire
    def __exit__(self, t: type[BaseException] | None, v: BaseException | None, tb: TracebackType | None) -> None: ...

RLock = _thread.RLock  # Actually a function at runtime.

class Condition:
    def __init__(self, lock: Lock | _RLock | RLock | None = None) -> None: ...
    def __enter__(self) -> bool: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> None: ...
    def acquire(self, blocking: bool = ..., timeout: float = ...) -> bool: ...
    def release(self) -> None: ...
    def wait(self, timeout: float | None = None) -> bool: ...
    def wait_for(self, predicate: Callable[[], _T], timeout: float | None = None) -> _T: ...
    def notify(self, n: int = 1) -> None: ...
    def notify_all(self) -> None: ...
    def notifyAll(self) -> None: ...  # deprecated alias for notify_all()

class Semaphore:
    _value: int
    def __init__(self, value: int = 1) -> None: ...
    def __exit__(self, t: type[BaseException] | None, v: BaseException | None, tb: TracebackType | None) -> None: ...
    def acquire(self, blocking: bool = True, timeout: float | None = None) -> bool: ...
    def __enter__(self, blocking: bool = True, timeout: float | None = None) -> bool: ...
    def release(self, n: int = 1) -> None: ...

class BoundedSemaphore(Semaphore): ...

class Event:
    def is_set(self) -> bool: ...
    def isSet(self) -> bool: ...  # deprecated alias for is_set()
    def set(self) -> None: ...
    def clear(self) -> None: ...
    def wait(self, timeout: float | None = None) -> bool: ...

excepthook = _excepthook
ExceptHookArgs = _ExceptHookArgs

class Timer(Thread):
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
    def cancel(self) -> None: ...

class Barrier:
    @property
    def parties(self) -> int: ...
    @property
    def n_waiting(self) -> int: ...
    @property
    def broken(self) -> bool: ...
    def __init__(self, parties: int, action: Callable[[], None] | None = None, timeout: float | None = None) -> None: ...
    def wait(self, timeout: float | None = None) -> int: ...
    def reset(self) -> None: ...
    def abort(self) -> None: ...

class BrokenBarrierError(RuntimeError): ...

import sys
import threading
from _typeshed import ConvertibleToInt, Incomplete, Unused
from collections.abc import Callable, Iterable, Mapping, MutableMapping, Sequence
from logging import Logger, _Level as _LoggingLevel
from typing import Any, Final, Generic, TypeVar, overload

__all__ = [
    "sub_debug",
    "debug",
    "info",
    "sub_warning",
    "get_logger",
    "log_to_stderr",
    "get_temp_dir",
    "register_after_fork",
    "is_exiting",
    "Finalize",
    "ForkAwareThreadLock",
    "ForkAwareLocal",
    "close_all_fds_except",
    "SUBDEBUG",
    "SUBWARNING",
]

if sys.version_info >= (3, 14):
    __all__ += ["warn"]

_T = TypeVar("_T")
_R_co = TypeVar("_R_co", default=Any, covariant=True)

NOTSET: Final = 0
SUBDEBUG: Final = 5
DEBUG: Final = 10
INFO: Final = 20
SUBWARNING: Final = 25
if sys.version_info >= (3, 14):
    WARNING: Final = 30

LOGGER_NAME: Final[str]
DEFAULT_LOGGING_FORMAT: Final[str]

def sub_debug(msg: object, *args: object) -> None: ...
def debug(msg: object, *args: object) -> None: ...
def info(msg: object, *args: object) -> None: ...

if sys.version_info >= (3, 14):
    def warn(msg: object, *args: object) -> None: ...

def sub_warning(msg: object, *args: object) -> None: ...
def get_logger() -> Logger: ...
def log_to_stderr(level: _LoggingLevel | None = None) -> Logger: ...
def is_abstract_socket_namespace(address: str | bytes | None) -> bool: ...

abstract_sockets_supported: Final[bool]

def get_temp_dir() -> str: ...
def register_after_fork(obj: _T, func: Callable[[_T], object]) -> None: ...

class Finalize(Generic[_R_co]):
    # "args" and "kwargs" are passed as arguments to "callback".
    @overload
    def __init__(
        self,
        obj: None,
        callback: Callable[..., _R_co],
        *,
        args: Sequence[Any] = (),
        kwargs: Mapping[str, Any] | None = None,
        exitpriority: int,
    ) -> None: ...
    @overload
    def __init__(
        self, obj: None, callback: Callable[..., _R_co], args: Sequence[Any], kwargs: Mapping[str, Any] | None, exitpriority: int
    ) -> None: ...
    @overload
    def __init__(
        self,
        obj: Any,
        callback: Callable[..., _R_co],
        args: Sequence[Any] = (),
        kwargs: Mapping[str, Any] | None = None,
        exitpriority: int | None = None,
    ) -> None: ...
    def __call__(
        self,
        wr: Unused = None,
        _finalizer_registry: MutableMapping[Incomplete, Incomplete] = {},
        sub_debug: Callable[..., object] = ...,
        getpid: Callable[[], int] = ...,
    ) -> _R_co: ...
    def cancel(self) -> None: ...
    def still_active(self) -> bool: ...

def is_exiting() -> bool: ...

class ForkAwareThreadLock:
    acquire: Callable[[bool, float], bool]
    release: Callable[[], None]
    def __enter__(self) -> bool: ...
    def __exit__(self, *args: Unused) -> None: ...

class ForkAwareLocal(threading.local): ...

MAXFD: Final[int]

def close_all_fds_except(fds: Iterable[int]) -> None: ...
def spawnv_passfds(path: bytes, args: Sequence[ConvertibleToInt], passfds: Sequence[int]) -> int: ...

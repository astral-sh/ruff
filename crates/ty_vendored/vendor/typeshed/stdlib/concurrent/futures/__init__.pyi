import sys

from ._base import (
    ALL_COMPLETED as ALL_COMPLETED,
    FIRST_COMPLETED as FIRST_COMPLETED,
    FIRST_EXCEPTION as FIRST_EXCEPTION,
    BrokenExecutor as BrokenExecutor,
    CancelledError as CancelledError,
    Executor as Executor,
    Future as Future,
    InvalidStateError as InvalidStateError,
    TimeoutError as TimeoutError,
    as_completed as as_completed,
    wait as wait,
)
from .process import ProcessPoolExecutor as ProcessPoolExecutor
from .thread import ThreadPoolExecutor as ThreadPoolExecutor

if sys.version_info >= (3, 14):
    from .interpreter import InterpreterPoolExecutor as InterpreterPoolExecutor

    __all__ = (
        "FIRST_COMPLETED",
        "FIRST_EXCEPTION",
        "ALL_COMPLETED",
        "CancelledError",
        "TimeoutError",
        "InvalidStateError",
        "BrokenExecutor",
        "Future",
        "Executor",
        "wait",
        "as_completed",
        "ProcessPoolExecutor",
        "ThreadPoolExecutor",
        "InterpreterPoolExecutor",
    )

elif sys.version_info >= (3, 13):
    __all__ = (
        "FIRST_COMPLETED",
        "FIRST_EXCEPTION",
        "ALL_COMPLETED",
        "CancelledError",
        "TimeoutError",
        "InvalidStateError",
        "BrokenExecutor",
        "Future",
        "Executor",
        "wait",
        "as_completed",
        "ProcessPoolExecutor",
        "ThreadPoolExecutor",
    )
else:
    __all__ = (
        "FIRST_COMPLETED",
        "FIRST_EXCEPTION",
        "ALL_COMPLETED",
        "CancelledError",
        "TimeoutError",
        "BrokenExecutor",
        "Future",
        "Executor",
        "wait",
        "as_completed",
        "ProcessPoolExecutor",
        "ThreadPoolExecutor",
    )

def __dir__() -> tuple[str, ...]: ...

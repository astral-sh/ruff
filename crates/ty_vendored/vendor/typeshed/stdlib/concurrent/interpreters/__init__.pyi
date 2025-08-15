import sys
import threading
import types
from collections.abc import Callable
from typing import Any, Literal, TypeVar
from typing_extensions import ParamSpec, Self

if sys.version_info >= (3, 13):  # needed to satisfy pyright checks for Python <3.13
    from _interpreters import (
        InterpreterError as InterpreterError,
        InterpreterNotFoundError as InterpreterNotFoundError,
        NotShareableError as NotShareableError,
        _SharedDict,
        _Whence,
        is_shareable as is_shareable,
    )

    from ._queues import Queue as Queue, QueueEmpty as QueueEmpty, QueueFull as QueueFull, create as create_queue

    __all__ = [
        "ExecutionFailed",
        "Interpreter",
        "InterpreterError",
        "InterpreterNotFoundError",
        "NotShareableError",
        "Queue",
        "QueueEmpty",
        "QueueFull",
        "create",
        "create_queue",
        "get_current",
        "get_main",
        "is_shareable",
        "list_all",
    ]

    _R = TypeVar("_R")
    _P = ParamSpec("_P")

    class ExecutionFailed(InterpreterError):
        excinfo: types.SimpleNamespace

        def __init__(self, excinfo: types.SimpleNamespace) -> None: ...

    def create() -> Interpreter: ...
    def list_all() -> list[Interpreter]: ...
    def get_current() -> Interpreter: ...
    def get_main() -> Interpreter: ...

    class Interpreter:
        def __new__(cls, id: int, /, _whence: _Whence | None = None, _ownsref: bool | None = None) -> Self: ...
        def __reduce__(self) -> tuple[type[Self], int]: ...
        def __hash__(self) -> int: ...
        def __del__(self) -> None: ...
        @property
        def id(self) -> int: ...
        @property
        def whence(
            self,
        ) -> Literal["unknown", "runtime init", "legacy C-API", "C-API", "cross-interpreter C-API", "_interpreters module"]: ...
        def is_running(self) -> bool: ...
        def close(self) -> None: ...
        def prepare_main(
            self, ns: _SharedDict | None = None, /, **kwargs: Any
        ) -> None: ...  # kwargs has same value restrictions as _SharedDict
        def exec(self, code: str | types.CodeType | Callable[[], object], /) -> None: ...
        def call(self, callable: Callable[_P, _R], /, *args: _P.args, **kwargs: _P.kwargs) -> _R: ...
        def call_in_thread(self, callable: Callable[_P, object], /, *args: _P.args, **kwargs: _P.kwargs) -> threading.Thread: ...

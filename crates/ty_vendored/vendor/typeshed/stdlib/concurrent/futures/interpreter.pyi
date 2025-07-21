import sys
from collections.abc import Callable, Mapping
from concurrent.futures import ThreadPoolExecutor
from typing import Literal, Protocol, overload, type_check_only
from typing_extensions import ParamSpec, Self, TypeAlias, TypeVar, TypeVarTuple, Unpack

_Task: TypeAlias = tuple[bytes, Literal["function", "script"]]

@type_check_only
class _TaskFunc(Protocol):
    @overload
    def __call__(self, fn: Callable[_P, _R], *args: _P.args, **kwargs: _P.kwargs) -> tuple[bytes, Literal["function"]]: ...
    @overload
    def __call__(self, fn: str) -> tuple[bytes, Literal["script"]]: ...

_Ts = TypeVarTuple("_Ts")
_P = ParamSpec("_P")
_R = TypeVar("_R")

# A `type.simplenamespace` with `__name__` attribute.
@type_check_only
class _HasName(Protocol):
    __name__: str

# `_interpreters.exec` technically gives us a simple namespace.
@type_check_only
class _ExcInfo(Protocol):
    formatted: str
    msg: str
    type: _HasName

if sys.version_info >= (3, 14):
    from concurrent.futures.thread import BrokenThreadPool, WorkerContext as ThreadWorkerContext

    from _interpreters import InterpreterError

    class ExecutionFailed(InterpreterError):
        def __init__(self, excinfo: _ExcInfo) -> None: ...  #  type: ignore[override]

    class WorkerContext(ThreadWorkerContext):
        # Parent class doesn't have `shared` argument,
        @overload  #  type: ignore[override]
        @classmethod
        def prepare(
            cls, initializer: Callable[[Unpack[_Ts]], object], initargs: tuple[Unpack[_Ts]], shared: Mapping[str, object]
        ) -> tuple[Callable[[], Self], _TaskFunc]: ...
        @overload  #  type: ignore[override]
        @classmethod
        def prepare(
            cls, initializer: Callable[[], object], initargs: tuple[()], shared: Mapping[str, object]
        ) -> tuple[Callable[[], Self], _TaskFunc]: ...
        def __init__(
            self, initdata: tuple[bytes, Literal["function", "script"]], shared: Mapping[str, object] | None = None
        ) -> None: ...  #  type: ignore[override]
        def __del__(self) -> None: ...
        def run(self, task: _Task) -> None: ...  #  type: ignore[override]

    class BrokenInterpreterPool(BrokenThreadPool): ...

    class InterpreterPoolExecutor(ThreadPoolExecutor):
        BROKEN: type[BrokenInterpreterPool]

        @overload  #  type: ignore[override]
        @classmethod
        def prepare_context(
            cls, initializer: Callable[[], object], initargs: tuple[()], shared: Mapping[str, object]
        ) -> tuple[Callable[[], WorkerContext], _TaskFunc]: ...
        @overload  #  type: ignore[override]
        @classmethod
        def prepare_context(
            cls, initializer: Callable[[Unpack[_Ts]], object], initargs: tuple[Unpack[_Ts]], shared: Mapping[str, object]
        ) -> tuple[Callable[[], WorkerContext], _TaskFunc]: ...
        @overload
        def __init__(
            self,
            max_workers: int | None = None,
            thread_name_prefix: str = "",
            initializer: Callable[[], object] | None = None,
            initargs: tuple[()] = (),
            shared: Mapping[str, object] | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self,
            max_workers: int | None = None,
            thread_name_prefix: str = "",
            *,
            initializer: Callable[[Unpack[_Ts]], object],
            initargs: tuple[Unpack[_Ts]],
            shared: Mapping[str, object] | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self,
            max_workers: int | None,
            thread_name_prefix: str,
            initializer: Callable[[Unpack[_Ts]], object],
            initargs: tuple[Unpack[_Ts]],
            shared: Mapping[str, object] | None = None,
        ) -> None: ...

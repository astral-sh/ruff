import sys
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from typing import Any, Literal, Protocol, overload, type_check_only
from typing_extensions import ParamSpec, Self, TypeAlias, TypeVar, TypeVarTuple, Unpack

_Task: TypeAlias = tuple[bytes, Literal["function", "script"]]
_Ts = TypeVarTuple("_Ts")
_P = ParamSpec("_P")
_R = TypeVar("_R")

@type_check_only
class _TaskFunc(Protocol):
    @overload
    def __call__(self, fn: Callable[_P, _R], *args: _P.args, **kwargs: _P.kwargs) -> tuple[bytes, Literal["function"]]: ...
    @overload
    def __call__(self, fn: str) -> tuple[bytes, Literal["script"]]: ...

if sys.version_info >= (3, 14):
    from concurrent.futures.thread import BrokenThreadPool, WorkerContext as ThreadWorkerContext
    from concurrent.interpreters import Interpreter, Queue

    def do_call(results: Queue, func: Callable[..., _R], args: tuple[Any, ...], kwargs: dict[str, Any]) -> _R: ...

    class WorkerContext(ThreadWorkerContext):
        interp: Interpreter | None
        results: Queue | None
        @overload  # type: ignore[override]
        @classmethod
        def prepare(
            cls, initializer: Callable[[Unpack[_Ts]], object], initargs: tuple[Unpack[_Ts]]
        ) -> tuple[Callable[[], Self], _TaskFunc]: ...
        @overload
        @classmethod
        def prepare(cls, initializer: Callable[[], object], initargs: tuple[()]) -> tuple[Callable[[], Self], _TaskFunc]: ...
        def __init__(self, initdata: _Task) -> None: ...
        def __del__(self) -> None: ...
        def run(self, task: _Task) -> None: ...  # type: ignore[override]

    class BrokenInterpreterPool(BrokenThreadPool): ...

    class InterpreterPoolExecutor(ThreadPoolExecutor):
        BROKEN: type[BrokenInterpreterPool]

        @overload  # type: ignore[override]
        @classmethod
        def prepare_context(
            cls, initializer: Callable[[], object], initargs: tuple[()]
        ) -> tuple[Callable[[], WorkerContext], _TaskFunc]: ...
        @overload
        @classmethod
        def prepare_context(
            cls, initializer: Callable[[Unpack[_Ts]], object], initargs: tuple[Unpack[_Ts]]
        ) -> tuple[Callable[[], WorkerContext], _TaskFunc]: ...
        @overload
        def __init__(
            self,
            max_workers: int | None = None,
            thread_name_prefix: str = "",
            initializer: Callable[[], object] | None = None,
            initargs: tuple[()] = (),
        ) -> None: ...
        @overload
        def __init__(
            self,
            max_workers: int | None = None,
            thread_name_prefix: str = "",
            *,
            initializer: Callable[[Unpack[_Ts]], object],
            initargs: tuple[Unpack[_Ts]],
        ) -> None: ...
        @overload
        def __init__(
            self,
            max_workers: int | None,
            thread_name_prefix: str,
            initializer: Callable[[Unpack[_Ts]], object],
            initargs: tuple[Unpack[_Ts]],
        ) -> None: ...

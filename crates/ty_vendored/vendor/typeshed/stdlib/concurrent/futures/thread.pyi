import queue
import sys
from collections.abc import Callable, Iterable, Mapping, Set as AbstractSet
from threading import Lock, Semaphore, Thread
from types import GenericAlias
from typing import Any, Generic, Protocol, TypeVar, overload, type_check_only
from typing_extensions import Self, TypeAlias, TypeVarTuple, Unpack
from weakref import ref

from ._base import BrokenExecutor, Executor, Future

_Ts = TypeVarTuple("_Ts")

_threads_queues: Mapping[Any, Any]
_shutdown: bool
_global_shutdown_lock: Lock

def _python_exit() -> None: ...

_S = TypeVar("_S")

_Task: TypeAlias = tuple[Callable[..., Any], tuple[Any, ...], dict[str, Any]]

_C = TypeVar("_C", bound=Callable[..., object])
_KT = TypeVar("_KT", bound=str)
_VT = TypeVar("_VT")

@type_check_only
class _ResolveTaskFunc(Protocol):
    def __call__(
        self, func: _C, args: tuple[Unpack[_Ts]], kwargs: dict[_KT, _VT]
    ) -> tuple[_C, tuple[Unpack[_Ts]], dict[_KT, _VT]]: ...

if sys.version_info >= (3, 14):
    class WorkerContext:
        @overload
        @classmethod
        def prepare(
            cls, initializer: Callable[[Unpack[_Ts]], object], initargs: tuple[Unpack[_Ts]]
        ) -> tuple[Callable[[], Self], _ResolveTaskFunc]: ...
        @overload
        @classmethod
        def prepare(
            cls, initializer: Callable[[], object], initargs: tuple[()]
        ) -> tuple[Callable[[], Self], _ResolveTaskFunc]: ...
        @overload
        def __init__(self, initializer: Callable[[Unpack[_Ts]], object], initargs: tuple[Unpack[_Ts]]) -> None: ...
        @overload
        def __init__(self, initializer: Callable[[], object], initargs: tuple[()]) -> None: ...
        def initialize(self) -> None: ...
        def finalize(self) -> None: ...
        def run(self, task: _Task) -> None: ...

if sys.version_info >= (3, 14):
    class _WorkItem(Generic[_S]):
        future: Future[Any]
        task: _Task
        def __init__(self, future: Future[Any], task: _Task) -> None: ...
        def run(self, ctx: WorkerContext) -> None: ...
        def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

    def _worker(executor_reference: ref[Any], ctx: WorkerContext, work_queue: queue.SimpleQueue[Any]) -> None: ...

else:
    class _WorkItem(Generic[_S]):
        future: Future[_S]
        fn: Callable[..., _S]
        args: Iterable[Any]
        kwargs: Mapping[str, Any]
        def __init__(self, future: Future[_S], fn: Callable[..., _S], args: Iterable[Any], kwargs: Mapping[str, Any]) -> None: ...
        def run(self) -> None: ...
        def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

    def _worker(
        executor_reference: ref[Any],
        work_queue: queue.SimpleQueue[Any],
        initializer: Callable[[Unpack[_Ts]], object],
        initargs: tuple[Unpack[_Ts]],
    ) -> None: ...

class BrokenThreadPool(BrokenExecutor): ...

class ThreadPoolExecutor(Executor):
    if sys.version_info >= (3, 14):
        BROKEN: type[BrokenThreadPool]

    _max_workers: int
    _idle_semaphore: Semaphore
    _threads: AbstractSet[Thread]
    _broken: bool
    _shutdown: bool
    _shutdown_lock: Lock
    _thread_name_prefix: str | None
    if sys.version_info >= (3, 14):
        _create_worker_context: Callable[[], WorkerContext]
        _resolve_work_item_task: _ResolveTaskFunc
    else:
        _initializer: Callable[..., None] | None
        _initargs: tuple[Any, ...]
    _work_queue: queue.SimpleQueue[_WorkItem[Any]]

    if sys.version_info >= (3, 14):
        @overload
        @classmethod
        def prepare_context(
            cls, initializer: Callable[[], object], initargs: tuple[()]
        ) -> tuple[Callable[[], WorkerContext], _ResolveTaskFunc]: ...
        @overload
        @classmethod
        def prepare_context(
            cls, initializer: Callable[[Unpack[_Ts]], object], initargs: tuple[Unpack[_Ts]]
        ) -> tuple[Callable[[], WorkerContext], _ResolveTaskFunc]: ...

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
    def _adjust_thread_count(self) -> None: ...
    def _initializer_failed(self) -> None: ...

import concurrent.futures
import sys
from _asyncio import (
    Task as Task,
    _enter_task as _enter_task,
    _leave_task as _leave_task,
    _register_task as _register_task,
    _unregister_task as _unregister_task,
)
from collections.abc import AsyncIterator, Awaitable, Coroutine, Generator, Iterable, Iterator
from typing import Any, Literal, Protocol, TypeVar, overload
from typing_extensions import TypeAlias

from . import _CoroutineLike
from .events import AbstractEventLoop
from .futures import Future

if sys.version_info >= (3, 11):
    from contextvars import Context

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.version_info >= (3, 12):
    __all__ = (
        "Task",
        "create_task",
        "FIRST_COMPLETED",
        "FIRST_EXCEPTION",
        "ALL_COMPLETED",
        "wait",
        "wait_for",
        "as_completed",
        "sleep",
        "gather",
        "shield",
        "ensure_future",
        "run_coroutine_threadsafe",
        "current_task",
        "all_tasks",
        "create_eager_task_factory",
        "eager_task_factory",
        "_register_task",
        "_unregister_task",
        "_enter_task",
        "_leave_task",
    )
else:
    __all__ = (
        "Task",
        "create_task",
        "FIRST_COMPLETED",
        "FIRST_EXCEPTION",
        "ALL_COMPLETED",
        "wait",
        "wait_for",
        "as_completed",
        "sleep",
        "gather",
        "shield",
        "ensure_future",
        "run_coroutine_threadsafe",
        "current_task",
        "all_tasks",
        "_register_task",
        "_unregister_task",
        "_enter_task",
        "_leave_task",
    )

_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)
_T1 = TypeVar("_T1")
_T2 = TypeVar("_T2")
_T3 = TypeVar("_T3")
_T4 = TypeVar("_T4")
_T5 = TypeVar("_T5")
_T6 = TypeVar("_T6")
_FT = TypeVar("_FT", bound=Future[Any])
if sys.version_info >= (3, 12):
    _FutureLike: TypeAlias = Future[_T] | Awaitable[_T]
else:
    _FutureLike: TypeAlias = Future[_T] | Generator[Any, None, _T] | Awaitable[_T]

_TaskYieldType: TypeAlias = Future[object] | None

FIRST_COMPLETED = concurrent.futures.FIRST_COMPLETED
FIRST_EXCEPTION = concurrent.futures.FIRST_EXCEPTION
ALL_COMPLETED = concurrent.futures.ALL_COMPLETED

if sys.version_info >= (3, 13):
    class _SyncAndAsyncIterator(Iterator[_T_co], AsyncIterator[_T_co], Protocol[_T_co]): ...

    def as_completed(fs: Iterable[_FutureLike[_T]], *, timeout: float | None = None) -> _SyncAndAsyncIterator[Future[_T]]: ...

elif sys.version_info >= (3, 10):
    def as_completed(fs: Iterable[_FutureLike[_T]], *, timeout: float | None = None) -> Iterator[Future[_T]]: ...

else:
    def as_completed(
        fs: Iterable[_FutureLike[_T]], *, loop: AbstractEventLoop | None = None, timeout: float | None = None
    ) -> Iterator[Future[_T]]: ...

@overload
def ensure_future(coro_or_future: _FT, *, loop: AbstractEventLoop | None = None) -> _FT: ...  # type: ignore[overload-overlap]
@overload
def ensure_future(coro_or_future: Awaitable[_T], *, loop: AbstractEventLoop | None = None) -> Task[_T]: ...

# `gather()` actually returns a list with length equal to the number
# of tasks passed; however, Tuple is used similar to the annotation for
# zip() because typing does not support variadic type variables.  See
# typing PR #1550 for discussion.
#
# N.B. Having overlapping overloads is the only way to get acceptable type inference in all edge cases.
if sys.version_info >= (3, 10):
    @overload
    def gather(coro_or_future1: _FutureLike[_T1], /, *, return_exceptions: Literal[False] = False) -> Future[tuple[_T1]]: ...  # type: ignore[overload-overlap]
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1], coro_or_future2: _FutureLike[_T2], /, *, return_exceptions: Literal[False] = False
    ) -> Future[tuple[_T1, _T2]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        /,
        *,
        return_exceptions: Literal[False] = False,
    ) -> Future[tuple[_T1, _T2, _T3]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        /,
        *,
        return_exceptions: Literal[False] = False,
    ) -> Future[tuple[_T1, _T2, _T3, _T4]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        coro_or_future5: _FutureLike[_T5],
        /,
        *,
        return_exceptions: Literal[False] = False,
    ) -> Future[tuple[_T1, _T2, _T3, _T4, _T5]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        coro_or_future5: _FutureLike[_T5],
        coro_or_future6: _FutureLike[_T6],
        /,
        *,
        return_exceptions: Literal[False] = False,
    ) -> Future[tuple[_T1, _T2, _T3, _T4, _T5, _T6]]: ...
    @overload
    def gather(*coros_or_futures: _FutureLike[_T], return_exceptions: Literal[False] = False) -> Future[list[_T]]: ...  # type: ignore[overload-overlap]
    @overload
    def gather(coro_or_future1: _FutureLike[_T1], /, *, return_exceptions: bool) -> Future[tuple[_T1 | BaseException]]: ...
    @overload
    def gather(
        coro_or_future1: _FutureLike[_T1], coro_or_future2: _FutureLike[_T2], /, *, return_exceptions: bool
    ) -> Future[tuple[_T1 | BaseException, _T2 | BaseException]]: ...
    @overload
    def gather(
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        /,
        *,
        return_exceptions: bool,
    ) -> Future[tuple[_T1 | BaseException, _T2 | BaseException, _T3 | BaseException]]: ...
    @overload
    def gather(
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        /,
        *,
        return_exceptions: bool,
    ) -> Future[tuple[_T1 | BaseException, _T2 | BaseException, _T3 | BaseException, _T4 | BaseException]]: ...
    @overload
    def gather(
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        coro_or_future5: _FutureLike[_T5],
        /,
        *,
        return_exceptions: bool,
    ) -> Future[
        tuple[_T1 | BaseException, _T2 | BaseException, _T3 | BaseException, _T4 | BaseException, _T5 | BaseException]
    ]: ...
    @overload
    def gather(
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        coro_or_future5: _FutureLike[_T5],
        coro_or_future6: _FutureLike[_T6],
        /,
        *,
        return_exceptions: bool,
    ) -> Future[
        tuple[
            _T1 | BaseException,
            _T2 | BaseException,
            _T3 | BaseException,
            _T4 | BaseException,
            _T5 | BaseException,
            _T6 | BaseException,
        ]
    ]: ...
    @overload
    def gather(*coros_or_futures: _FutureLike[_T], return_exceptions: bool) -> Future[list[_T | BaseException]]: ...

else:
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1], /, *, loop: AbstractEventLoop | None = None, return_exceptions: Literal[False] = False
    ) -> Future[tuple[_T1]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        /,
        *,
        loop: AbstractEventLoop | None = None,
        return_exceptions: Literal[False] = False,
    ) -> Future[tuple[_T1, _T2]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        /,
        *,
        loop: AbstractEventLoop | None = None,
        return_exceptions: Literal[False] = False,
    ) -> Future[tuple[_T1, _T2, _T3]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        /,
        *,
        loop: AbstractEventLoop | None = None,
        return_exceptions: Literal[False] = False,
    ) -> Future[tuple[_T1, _T2, _T3, _T4]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        coro_or_future5: _FutureLike[_T5],
        /,
        *,
        loop: AbstractEventLoop | None = None,
        return_exceptions: Literal[False] = False,
    ) -> Future[tuple[_T1, _T2, _T3, _T4, _T5]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        coro_or_future5: _FutureLike[_T5],
        coro_or_future6: _FutureLike[_T6],
        /,
        *,
        loop: AbstractEventLoop | None = None,
        return_exceptions: Literal[False] = False,
    ) -> Future[tuple[_T1, _T2, _T3, _T4, _T5, _T6]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        *coros_or_futures: _FutureLike[_T], loop: AbstractEventLoop | None = None, return_exceptions: Literal[False] = False
    ) -> Future[list[_T]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1], /, *, loop: AbstractEventLoop | None = None, return_exceptions: bool
    ) -> Future[tuple[_T1 | BaseException]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        /,
        *,
        loop: AbstractEventLoop | None = None,
        return_exceptions: bool,
    ) -> Future[tuple[_T1 | BaseException, _T2 | BaseException]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        /,
        *,
        loop: AbstractEventLoop | None = None,
        return_exceptions: bool,
    ) -> Future[tuple[_T1 | BaseException, _T2 | BaseException, _T3 | BaseException]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        /,
        *,
        loop: AbstractEventLoop | None = None,
        return_exceptions: bool,
    ) -> Future[tuple[_T1 | BaseException, _T2 | BaseException, _T3 | BaseException, _T4 | BaseException]]: ...
    @overload
    def gather(  # type: ignore[overload-overlap]
        coro_or_future1: _FutureLike[_T1],
        coro_or_future2: _FutureLike[_T2],
        coro_or_future3: _FutureLike[_T3],
        coro_or_future4: _FutureLike[_T4],
        coro_or_future5: _FutureLike[_T5],
        coro_or_future6: _FutureLike[_T6],
        /,
        *,
        loop: AbstractEventLoop | None = None,
        return_exceptions: bool,
    ) -> Future[
        tuple[
            _T1 | BaseException,
            _T2 | BaseException,
            _T3 | BaseException,
            _T4 | BaseException,
            _T5 | BaseException,
            _T6 | BaseException,
        ]
    ]: ...
    @overload
    def gather(
        *coros_or_futures: _FutureLike[_T], loop: AbstractEventLoop | None = None, return_exceptions: bool
    ) -> Future[list[_T | BaseException]]: ...

# unlike some asyncio apis, This does strict runtime checking of actually being a coroutine, not of any future-like.
def run_coroutine_threadsafe(coro: Coroutine[Any, Any, _T], loop: AbstractEventLoop) -> concurrent.futures.Future[_T]: ...

if sys.version_info >= (3, 10):
    def shield(arg: _FutureLike[_T]) -> Future[_T]: ...
    @overload
    async def sleep(delay: float) -> None: ...
    @overload
    async def sleep(delay: float, result: _T) -> _T: ...
    async def wait_for(fut: _FutureLike[_T], timeout: float | None) -> _T: ...

else:
    def shield(arg: _FutureLike[_T], *, loop: AbstractEventLoop | None = None) -> Future[_T]: ...
    @overload
    async def sleep(delay: float, *, loop: AbstractEventLoop | None = None) -> None: ...
    @overload
    async def sleep(delay: float, result: _T, *, loop: AbstractEventLoop | None = None) -> _T: ...
    async def wait_for(fut: _FutureLike[_T], timeout: float | None, *, loop: AbstractEventLoop | None = None) -> _T: ...

if sys.version_info >= (3, 11):
    @overload
    async def wait(
        fs: Iterable[_FT], *, timeout: float | None = None, return_when: str = "ALL_COMPLETED"
    ) -> tuple[set[_FT], set[_FT]]: ...
    @overload
    async def wait(
        fs: Iterable[Task[_T]], *, timeout: float | None = None, return_when: str = "ALL_COMPLETED"
    ) -> tuple[set[Task[_T]], set[Task[_T]]]: ...

elif sys.version_info >= (3, 10):
    @overload
    async def wait(  # type: ignore[overload-overlap]
        fs: Iterable[_FT], *, timeout: float | None = None, return_when: str = "ALL_COMPLETED"
    ) -> tuple[set[_FT], set[_FT]]: ...
    @overload
    async def wait(
        fs: Iterable[Awaitable[_T]], *, timeout: float | None = None, return_when: str = "ALL_COMPLETED"
    ) -> tuple[set[Task[_T]], set[Task[_T]]]: ...

else:
    @overload
    async def wait(  # type: ignore[overload-overlap]
        fs: Iterable[_FT],
        *,
        loop: AbstractEventLoop | None = None,
        timeout: float | None = None,
        return_when: str = "ALL_COMPLETED",
    ) -> tuple[set[_FT], set[_FT]]: ...
    @overload
    async def wait(
        fs: Iterable[Awaitable[_T]],
        *,
        loop: AbstractEventLoop | None = None,
        timeout: float | None = None,
        return_when: str = "ALL_COMPLETED",
    ) -> tuple[set[Task[_T]], set[Task[_T]]]: ...

if sys.version_info >= (3, 12):
    _TaskCompatibleCoro: TypeAlias = Coroutine[Any, Any, _T_co]
elif sys.version_info >= (3, 9):
    _TaskCompatibleCoro: TypeAlias = Generator[_TaskYieldType, None, _T_co] | Coroutine[Any, Any, _T_co]
else:
    _TaskCompatibleCoro: TypeAlias = Generator[_TaskYieldType, None, _T_co] | Awaitable[_T_co]

def all_tasks(loop: AbstractEventLoop | None = None) -> set[Task[Any]]: ...

if sys.version_info >= (3, 11):
    def create_task(coro: _CoroutineLike[_T], *, name: str | None = None, context: Context | None = None) -> Task[_T]: ...

else:
    def create_task(coro: _CoroutineLike[_T], *, name: str | None = None) -> Task[_T]: ...

if sys.version_info >= (3, 12):
    from _asyncio import current_task as current_task
else:
    def current_task(loop: AbstractEventLoop | None = None) -> Task[Any] | None: ...

if sys.version_info >= (3, 12):
    _TaskT_co = TypeVar("_TaskT_co", bound=Task[Any], covariant=True)

    class _CustomTaskConstructor(Protocol[_TaskT_co]):
        def __call__(
            self,
            coro: _TaskCompatibleCoro[Any],
            /,
            *,
            loop: AbstractEventLoop,
            name: str | None,
            context: Context | None,
            eager_start: bool,
        ) -> _TaskT_co: ...

    class _EagerTaskFactoryType(Protocol[_TaskT_co]):
        def __call__(
            self,
            loop: AbstractEventLoop,
            coro: _TaskCompatibleCoro[Any],
            *,
            name: str | None = None,
            context: Context | None = None,
        ) -> _TaskT_co: ...

    def create_eager_task_factory(
        custom_task_constructor: _CustomTaskConstructor[_TaskT_co],
    ) -> _EagerTaskFactoryType[_TaskT_co]: ...
    def eager_task_factory(
        loop: AbstractEventLoop | None,
        coro: _TaskCompatibleCoro[_T_co],
        *,
        name: str | None = None,
        context: Context | None = None,
    ) -> Task[_T_co]: ...

"""Support for tasks, coroutines and the scheduler."""

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
from typing import Any, Final, Literal, Protocol, TypeVar, overload, type_check_only
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

FIRST_COMPLETED: Final = concurrent.futures.FIRST_COMPLETED
FIRST_EXCEPTION: Final = concurrent.futures.FIRST_EXCEPTION
ALL_COMPLETED: Final = concurrent.futures.ALL_COMPLETED

if sys.version_info >= (3, 13):
    @type_check_only
    class _SyncAndAsyncIterator(Iterator[_T_co], AsyncIterator[_T_co], Protocol[_T_co]): ...

    def as_completed(fs: Iterable[_FutureLike[_T]], *, timeout: float | None = None) -> _SyncAndAsyncIterator[Future[_T]]:
        """Create an iterator of awaitables or their results in completion order.

        Run the supplied awaitables concurrently. The returned object can be
        iterated to obtain the results of the awaitables as they finish.

        The object returned can be iterated as an asynchronous iterator or a plain
        iterator. When asynchronous iteration is used, the originally-supplied
        awaitables are yielded if they are tasks or futures. This makes it easy to
        correlate previously-scheduled tasks with their results:

            ipv4_connect = create_task(open_connection("127.0.0.1", 80))
            ipv6_connect = create_task(open_connection("::1", 80))
            tasks = [ipv4_connect, ipv6_connect]

            async for earliest_connect in as_completed(tasks):
                # earliest_connect is done. The result can be obtained by
                # awaiting it or calling earliest_connect.result()
                reader, writer = await earliest_connect

                if earliest_connect is ipv6_connect:
                    print("IPv6 connection established.")
                else:
                    print("IPv4 connection established.")

        During asynchronous iteration, implicitly-created tasks will be yielded for
        supplied awaitables that aren't tasks or futures.

        When used as a plain iterator, each iteration yields a new coroutine that
        returns the result or raises the exception of the next completed awaitable.
        This pattern is compatible with Python versions older than 3.13:

            ipv4_connect = create_task(open_connection("127.0.0.1", 80))
            ipv6_connect = create_task(open_connection("::1", 80))
            tasks = [ipv4_connect, ipv6_connect]

            for next_connect in as_completed(tasks):
                # next_connect is not one of the original task objects. It must be
                # awaited to obtain the result value or raise the exception of the
                # awaitable that finishes next.
                reader, writer = await next_connect

        A TimeoutError is raised if the timeout occurs before all awaitables are
        done. This is raised by the async for loop during asynchronous iteration or
        by the coroutines yielded during plain iteration.
        """

elif sys.version_info >= (3, 10):
    def as_completed(fs: Iterable[_FutureLike[_T]], *, timeout: float | None = None) -> Iterator[Future[_T]]:
        """Return an iterator whose values are coroutines.

        When waiting for the yielded coroutines you'll get the results (or
        exceptions!) of the original Futures (or coroutines), in the order
        in which and as soon as they complete.

        This differs from PEP 3148; the proper way to use this is:

            for f in as_completed(fs):
                result = await f  # The 'await' may raise.
                # Use result.

        If a timeout is specified, the 'await' will raise
        TimeoutError when the timeout occurs before all Futures are done.

        Note: The futures 'f' are not necessarily members of fs.
        """

else:
    def as_completed(
        fs: Iterable[_FutureLike[_T]], *, loop: AbstractEventLoop | None = None, timeout: float | None = None
    ) -> Iterator[Future[_T]]:
        """Return an iterator whose values are coroutines.

        When waiting for the yielded coroutines you'll get the results (or
        exceptions!) of the original Futures (or coroutines), in the order
        in which and as soon as they complete.

        This differs from PEP 3148; the proper way to use this is:

            for f in as_completed(fs):
                result = await f  # The 'await' may raise.
                # Use result.

        If a timeout is specified, the 'await' will raise
        TimeoutError when the timeout occurs before all Futures are done.

        Note: The futures 'f' are not necessarily members of fs.
        """

@overload
def ensure_future(coro_or_future: _FT, *, loop: AbstractEventLoop | None = None) -> _FT:  # type: ignore[overload-overlap]
    """Wrap a coroutine or an awaitable in a future.

    If the argument is a Future, it is returned directly.
    """

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
    def gather(coro_or_future1: _FutureLike[_T1], /, *, return_exceptions: Literal[False] = False) -> Future[tuple[_T1]]:  # type: ignore[overload-overlap]
        """Return a future aggregating results from the given coroutines/futures.

        Coroutines will be wrapped in a future and scheduled in the event
        loop. They will not necessarily be scheduled in the same order as
        passed in.

        All futures must share the same event loop.  If all the tasks are
        done successfully, the returned future's result is the list of
        results (in the order of the original sequence, not necessarily
        the order of results arrival).  If *return_exceptions* is True,
        exceptions in the tasks are treated the same as successful
        results, and gathered in the result list; otherwise, the first
        raised exception will be immediately propagated to the returned
        future.

        Cancellation: if the outer Future is cancelled, all children (that
        have not completed yet) are also cancelled.  If any child is
        cancelled, this is treated as if it raised CancelledError --
        the outer Future is *not* cancelled in this case.  (This is to
        prevent the cancellation of one child to cause other children to
        be cancelled.)

        If *return_exceptions* is False, cancelling gather() after it
        has been marked done won't cancel any submitted awaitables.
        For instance, gather can be marked done after propagating an
        exception to the caller, therefore, calling ``gather.cancel()``
        after catching an exception (raised by one of the awaitables) from
        gather won't cancel any other awaitables.
        """

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
    ) -> Future[tuple[_T1]]:
        """Return a future aggregating results from the given coroutines/futures.

        Coroutines will be wrapped in a future and scheduled in the event
        loop. They will not necessarily be scheduled in the same order as
        passed in.

        All futures must share the same event loop.  If all the tasks are
        done successfully, the returned future's result is the list of
        results (in the order of the original sequence, not necessarily
        the order of results arrival).  If *return_exceptions* is True,
        exceptions in the tasks are treated the same as successful
        results, and gathered in the result list; otherwise, the first
        raised exception will be immediately propagated to the returned
        future.

        Cancellation: if the outer Future is cancelled, all children (that
        have not completed yet) are also cancelled.  If any child is
        cancelled, this is treated as if it raised CancelledError --
        the outer Future is *not* cancelled in this case.  (This is to
        prevent the cancellation of one child to cause other children to
        be cancelled.)

        If *return_exceptions* is False, cancelling gather() after it
        has been marked done won't cancel any submitted awaitables.
        For instance, gather can be marked done after propagating an
        exception to the caller, therefore, calling ``gather.cancel()``
        after catching an exception (raised by one of the awaitables) from
        gather won't cancel any other awaitables.
        """

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
def run_coroutine_threadsafe(coro: Coroutine[Any, Any, _T], loop: AbstractEventLoop) -> concurrent.futures.Future[_T]:
    """Submit a coroutine object to a given event loop.

    Return a concurrent.futures.Future to access the result.
    """

if sys.version_info >= (3, 10):
    def shield(arg: _FutureLike[_T]) -> Future[_T]:
        """Wait for a future, shielding it from cancellation.

        The statement

            task = asyncio.create_task(something())
            res = await shield(task)

        is exactly equivalent to the statement

            res = await something()

        *except* that if the coroutine containing it is cancelled, the
        task running in something() is not cancelled.  From the POV of
        something(), the cancellation did not happen.  But its caller is
        still cancelled, so the yield-from expression still raises
        CancelledError.  Note: If something() is cancelled by other means
        this will still cancel shield().

        If you want to completely ignore cancellation (not recommended)
        you can combine shield() with a try/except clause, as follows:

            task = asyncio.create_task(something())
            try:
                res = await shield(task)
            except CancelledError:
                res = None

        Save a reference to tasks passed to this function, to avoid
        a task disappearing mid-execution. The event loop only keeps
        weak references to tasks. A task that isn't referenced elsewhere
        may get garbage collected at any time, even before it's done.
        """

    @overload
    async def sleep(delay: float) -> None:
        """Coroutine that completes after a given time (in seconds)."""

    @overload
    async def sleep(delay: float, result: _T) -> _T: ...
    async def wait_for(fut: _FutureLike[_T], timeout: float | None) -> _T:
        """Wait for the single Future or coroutine to complete, with timeout.

        Coroutine will be wrapped in Task.

        Returns result of the Future or coroutine.  When a timeout occurs,
        it cancels the task and raises TimeoutError.  To avoid the task
        cancellation, wrap it in shield().

        If the wait is cancelled, the task is also cancelled.

        If the task suppresses the cancellation and returns a value instead,
        that value is returned.

        This function is a coroutine.
        """

else:
    def shield(arg: _FutureLike[_T], *, loop: AbstractEventLoop | None = None) -> Future[_T]:
        """Wait for a future, shielding it from cancellation.

        The statement

            res = await shield(something())

        is exactly equivalent to the statement

            res = await something()

        *except* that if the coroutine containing it is cancelled, the
        task running in something() is not cancelled.  From the POV of
        something(), the cancellation did not happen.  But its caller is
        still cancelled, so the yield-from expression still raises
        CancelledError.  Note: If something() is cancelled by other means
        this will still cancel shield().

        If you want to completely ignore cancellation (not recommended)
        you can combine shield() with a try/except clause, as follows:

            try:
                res = await shield(something())
            except CancelledError:
                res = None
        """

    @overload
    async def sleep(delay: float, *, loop: AbstractEventLoop | None = None) -> None:
        """Coroutine that completes after a given time (in seconds)."""

    @overload
    async def sleep(delay: float, result: _T, *, loop: AbstractEventLoop | None = None) -> _T: ...
    async def wait_for(fut: _FutureLike[_T], timeout: float | None, *, loop: AbstractEventLoop | None = None) -> _T:
        """Wait for the single Future or coroutine to complete, with timeout.

        Coroutine will be wrapped in Task.

        Returns result of the Future or coroutine.  When a timeout occurs,
        it cancels the task and raises TimeoutError.  To avoid the task
        cancellation, wrap it in shield().

        If the wait is cancelled, the task is also cancelled.

        This function is a coroutine.
        """

if sys.version_info >= (3, 11):
    @overload
    async def wait(
        fs: Iterable[_FT], *, timeout: float | None = None, return_when: str = "ALL_COMPLETED"
    ) -> tuple[set[_FT], set[_FT]]:
        """Wait for the Futures or Tasks given by fs to complete.

        The fs iterable must not be empty.

        Returns two sets of Future: (done, pending).

        Usage:

            done, pending = await asyncio.wait(fs)

        Note: This does not raise TimeoutError! Futures that aren't done
        when the timeout occurs are returned in the second set.
        """

    @overload
    async def wait(
        fs: Iterable[Task[_T]], *, timeout: float | None = None, return_when: str = "ALL_COMPLETED"
    ) -> tuple[set[Task[_T]], set[Task[_T]]]: ...

elif sys.version_info >= (3, 10):
    @overload
    async def wait(  # type: ignore[overload-overlap]
        fs: Iterable[_FT], *, timeout: float | None = None, return_when: str = "ALL_COMPLETED"
    ) -> tuple[set[_FT], set[_FT]]:
        """Wait for the Futures and coroutines given by fs to complete.

        The fs iterable must not be empty.

        Coroutines will be wrapped in Tasks.

        Returns two sets of Future: (done, pending).

        Usage:

            done, pending = await asyncio.wait(fs)

        Note: This does not raise TimeoutError! Futures that aren't done
        when the timeout occurs are returned in the second set.
        """

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
    ) -> tuple[set[_FT], set[_FT]]:
        """Wait for the Futures and coroutines given by fs to complete.

        The fs iterable must not be empty.

        Coroutines will be wrapped in Tasks.

        Returns two sets of Future: (done, pending).

        Usage:

            done, pending = await asyncio.wait(fs)

        Note: This does not raise TimeoutError! Futures that aren't done
        when the timeout occurs are returned in the second set.
        """

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
else:
    _TaskCompatibleCoro: TypeAlias = Generator[_TaskYieldType, None, _T_co] | Coroutine[Any, Any, _T_co]

def all_tasks(loop: AbstractEventLoop | None = None) -> set[Task[Any]]:
    """Return a set of all tasks for the loop."""

if sys.version_info >= (3, 11):
    def create_task(coro: _CoroutineLike[_T], *, name: str | None = None, context: Context | None = None) -> Task[_T]:
        """Schedule the execution of a coroutine object in a spawn task.

        Return a Task object.
        """

else:
    def create_task(coro: _CoroutineLike[_T], *, name: str | None = None) -> Task[_T]:
        """Schedule the execution of a coroutine object in a spawn task.

        Return a Task object.
        """

if sys.version_info >= (3, 12):
    from _asyncio import current_task as current_task
else:
    def current_task(loop: AbstractEventLoop | None = None) -> Task[Any] | None:
        """Return a currently executed task."""

if sys.version_info >= (3, 14):
    def eager_task_factory(
        loop: AbstractEventLoop | None,
        coro: _TaskCompatibleCoro[_T_co],
        *,
        name: str | None = None,
        context: Context | None = None,
        eager_start: bool = True,
    ) -> Task[_T_co]: ...

elif sys.version_info >= (3, 12):
    def eager_task_factory(
        loop: AbstractEventLoop | None,
        coro: _TaskCompatibleCoro[_T_co],
        *,
        name: str | None = None,
        context: Context | None = None,
    ) -> Task[_T_co]: ...

if sys.version_info >= (3, 12):
    _TaskT_co = TypeVar("_TaskT_co", bound=Task[Any], covariant=True)

    @type_check_only
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

    @type_check_only
    class _EagerTaskFactoryType(Protocol[_TaskT_co]):
        def __call__(
            self,
            loop: AbstractEventLoop,
            coro: _TaskCompatibleCoro[Any],
            *,
            name: str | None = None,
            context: Context | None = None,
        ) -> _TaskT_co: ...

    def create_eager_task_factory(custom_task_constructor: _CustomTaskConstructor[_TaskT_co]) -> _EagerTaskFactoryType[_TaskT_co]:
        """Create a function suitable for use as a task factory on an event-loop.

        Example usage:

            loop.set_task_factory(
                asyncio.create_eager_task_factory(my_task_constructor))

        Now, tasks created will be started immediately (rather than being first
        scheduled to an event loop). The constructor argument can be any callable
        that returns a Task-compatible object and has a signature compatible
        with `Task.__init__`; it must have the `eager_start` keyword argument.

        Most applications will use `Task` for `custom_task_constructor` and in
        this case there's no need to call `create_eager_task_factory()`
        directly. Instead the  global `eager_task_factory` instance can be
        used. E.g. `loop.set_task_factory(asyncio.eager_task_factory)`.
        """

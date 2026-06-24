import sys
from contextvars import Context
from types import TracebackType
from typing import Any, TypeVar
from typing_extensions import Self

from . import _CoroutineLike
from .events import AbstractEventLoop
from .tasks import Task

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.version_info >= (3, 12):
    __all__ = ("TaskGroup",)
else:
    __all__ = ["TaskGroup"]

_T = TypeVar("_T")

class TaskGroup:
    """Asynchronous context manager for managing groups of tasks.

    Example use:

        async with asyncio.TaskGroup() as group:
            task1 = group.create_task(some_coroutine(...))
            task2 = group.create_task(other_coroutine(...))
        print("Both tasks have completed now.")

    All tasks are awaited when the context manager exits.

    Any exceptions other than `asyncio.CancelledError` raised within
    a task will cancel all remaining tasks and wait for them to exit.
    The exceptions are then combined and raised as an `ExceptionGroup`.
    """

    _loop: AbstractEventLoop | None
    _tasks: set[Task[Any]]

    async def __aenter__(self) -> Self: ...
    async def __aexit__(self, et: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None) -> None: ...
    if sys.version_info >= (3, 14):
        def create_task(
            self,
            coro: _CoroutineLike[_T],
            *,
            name: str | None = None,
            context: Context | None = None,
            eager_start: bool | None = None,
        ) -> Task[_T]:
            """Create a new task in this group and return it.

            Similar to `asyncio.create_task`.
            """

    else:
        def create_task(self, coro: _CoroutineLike[_T], *, name: str | None = None, context: Context | None = None) -> Task[_T]:
            """Create a new task in this group and return it.

            Similar to `asyncio.create_task`.
            """

    def _on_task_done(self, task: Task[object]) -> None: ...
    if sys.version_info >= (3, 15):
        def cancel(self) -> None:
            """Cancel the task group

            `cancel()` will be called on any tasks in the group that aren't yet
            done, as well as the parent (body) of the group.  This will cause the
            task group context manager to exit *without* `asyncio.CancelledError`
            being raised.

            If `cancel()` is called before entering the task group, the group will be
            cancelled upon entry.  This is useful for patterns where one piece of
            code passes an unused TaskGroup instance to another in order to have
            the ability to cancel anything run within the group.

            `cancel()` is idempotent and may be called after the task group has
            already exited.
            """

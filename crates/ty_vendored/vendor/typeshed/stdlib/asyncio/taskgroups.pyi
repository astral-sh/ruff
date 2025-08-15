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
    def create_task(self, coro: _CoroutineLike[_T], *, name: str | None = None, context: Context | None = None) -> Task[_T]:
        """Create a new task in this group and return it.

        Similar to `asyncio.create_task`.
        """

    def _on_task_done(self, task: Task[object]) -> None: ...

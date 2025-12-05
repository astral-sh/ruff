import sys
from _typeshed import Unused
from collections.abc import Awaitable, Callable, Coroutine
from contextvars import Context
from typing import Any, TypeVar, final
from typing_extensions import Self

from .events import AbstractEventLoop

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.version_info >= (3, 11):
    __all__ = ("Runner", "run")
else:
    __all__ = ("run",)
_T = TypeVar("_T")

if sys.version_info >= (3, 11):
    @final
    class Runner:
        """A context manager that controls event loop life cycle.

        The context manager always creates a new event loop,
        allows to run async functions inside it,
        and properly finalizes the loop at the context manager exit.

        If debug is True, the event loop will be run in debug mode.
        If loop_factory is passed, it is used for new event loop creation.

        asyncio.run(main(), debug=True)

        is a shortcut for

        with asyncio.Runner(debug=True) as runner:
            runner.run(main())

        The run() method can be called multiple times within the runner's context.

        This can be useful for interactive console (e.g. IPython),
        unittest runners, console tools, -- everywhere when async code
        is called from existing sync framework and where the preferred single
        asyncio.run() call doesn't work.

        """

        def __init__(self, *, debug: bool | None = None, loop_factory: Callable[[], AbstractEventLoop] | None = None) -> None: ...
        def __enter__(self) -> Self: ...
        def __exit__(self, exc_type: Unused, exc_val: Unused, exc_tb: Unused) -> None: ...
        def close(self) -> None:
            """Shutdown and close event loop."""

        def get_loop(self) -> AbstractEventLoop:
            """Return embedded event loop."""
        if sys.version_info >= (3, 14):
            def run(self, coro: Awaitable[_T], *, context: Context | None = None) -> _T:
                """Run code in the embedded event loop."""
        else:
            def run(self, coro: Coroutine[Any, Any, _T], *, context: Context | None = None) -> _T:
                """Run a coroutine inside the embedded event loop."""

if sys.version_info >= (3, 12):
    def run(
        main: Coroutine[Any, Any, _T], *, debug: bool | None = None, loop_factory: Callable[[], AbstractEventLoop] | None = None
    ) -> _T:
        """Execute the coroutine and return the result.

        This function runs the passed coroutine, taking care of
        managing the asyncio event loop, finalizing asynchronous
        generators and closing the default executor.

        This function cannot be called when another asyncio event loop is
        running in the same thread.

        If debug is True, the event loop will be run in debug mode.
        If loop_factory is passed, it is used for new event loop creation.

        This function always creates a new event loop and closes it at the end.
        It should be used as a main entry point for asyncio programs, and should
        ideally only be called once.

        The executor is given a timeout duration of 5 minutes to shutdown.
        If the executor hasn't finished within that duration, a warning is
        emitted and the executor is closed.

        Example:

            async def main():
                await asyncio.sleep(1)
                print('hello')

            asyncio.run(main())
        """

else:
    def run(main: Coroutine[Any, Any, _T], *, debug: bool | None = None) -> _T:
        """Execute the coroutine and return the result.

        This function runs the passed coroutine, taking care of
        managing the asyncio event loop and finalizing asynchronous
        generators.

        This function cannot be called when another asyncio event loop is
        running in the same thread.

        If debug is True, the event loop will be run in debug mode.

        This function always creates a new event loop and closes it at the end.
        It should be used as a main entry point for asyncio programs, and should
        ideally only be called once.

        Example:

            async def main():
                await asyncio.sleep(1)
                print('hello')

            asyncio.run(main())
        """

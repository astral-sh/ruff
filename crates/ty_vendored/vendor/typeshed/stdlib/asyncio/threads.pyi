"""High-level support for working with threads in asyncio"""

from collections.abc import Callable
from typing import TypeVar
from typing_extensions import ParamSpec

# Keep asyncio.__all__ updated with any changes to __all__ here
__all__ = ("to_thread",)
_P = ParamSpec("_P")
_R = TypeVar("_R")

async def to_thread(func: Callable[_P, _R], /, *args: _P.args, **kwargs: _P.kwargs) -> _R:
    """Asynchronously run function *func* in a separate thread.

    Any *args and **kwargs supplied for this function are directly passed
    to *func*. Also, the current :class:`contextvars.Context` is propagated,
    allowing context variables from the main thread to be accessed in the
    separate thread.

    Return a coroutine that can be awaited to get the eventual result of *func*.
    """

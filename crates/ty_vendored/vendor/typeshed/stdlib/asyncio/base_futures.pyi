from _asyncio import Future
from collections.abc import Callable, Sequence
from contextvars import Context
from typing import Any, Final
from typing_extensions import TypeIs

from . import futures

__all__ = ()

_PENDING: Final = "PENDING"  # undocumented
_CANCELLED: Final = "CANCELLED"  # undocumented
_FINISHED: Final = "FINISHED"  # undocumented

def isfuture(obj: object) -> TypeIs[Future[Any]]:
    """Check for a Future.

    This returns True when obj is a Future instance or is advertising
    itself as duck-type compatible by setting _asyncio_future_blocking.
    See comment in Future for more details.
    """

def _format_callbacks(cb: Sequence[tuple[Callable[[futures.Future[Any]], None], Context]]) -> str:  # undocumented
    """helper function for Future.__repr__"""

def _future_repr_info(future: futures.Future[Any]) -> list[str]:  # undocumented
    """helper function for Future.__repr__"""

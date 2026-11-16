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

def isfuture(obj: object) -> TypeIs[Future[Any]]: ...
def _format_callbacks(cb: Sequence[tuple[Callable[[futures.Future[Any]], None], Context]]) -> str: ...  # undocumented
def _future_repr_info(future: futures.Future[Any]) -> list[str]: ...  # undocumented

import queue
import sys
from typing import Final, SupportsIndex
from typing_extensions import Self

if sys.version_info >= (3, 13):  # needed to satisfy pyright checks for Python <3.13
    from _interpqueues import QueueError as QueueError, QueueNotFoundError as QueueNotFoundError

    from . import _crossinterp
    from ._crossinterp import UNBOUND_ERROR as UNBOUND_ERROR, UNBOUND_REMOVE as UNBOUND_REMOVE, UnboundItem, _AnyUnbound

    __all__ = [
        "UNBOUND",
        "UNBOUND_ERROR",
        "UNBOUND_REMOVE",
        "ItemInterpreterDestroyed",
        "Queue",
        "QueueEmpty",
        "QueueError",
        "QueueFull",
        "QueueNotFoundError",
        "create",
        "list_all",
    ]

    class QueueEmpty(QueueError, queue.Empty): ...
    class QueueFull(QueueError, queue.Full): ...
    class ItemInterpreterDestroyed(QueueError, _crossinterp.ItemInterpreterDestroyed): ...
    UNBOUND: Final[UnboundItem]

    def create(maxsize: int = 0, *, unbounditems: _AnyUnbound = ...) -> Queue: ...
    def list_all() -> list[Queue]: ...

    class Queue:
        def __new__(cls, id: int, /) -> Self: ...
        def __del__(self) -> None: ...
        def __hash__(self) -> int: ...
        def __reduce__(self) -> tuple[type[Self], int]: ...
        @property
        def id(self) -> int: ...
        @property
        def unbounditems(self) -> _AnyUnbound: ...
        @property
        def maxsize(self) -> int: ...
        def empty(self) -> bool: ...
        def full(self) -> bool: ...
        def qsize(self) -> int: ...
        if sys.version_info >= (3, 14):
            def put(
                self,
                obj: object,
                block: bool = True,
                timeout: SupportsIndex | None = None,
                *,
                unbounditems: _AnyUnbound | None = None,
                _delay: float = 0.01,
            ) -> None: ...
        else:
            def put(
                self,
                obj: object,
                timeout: SupportsIndex | None = None,
                *,
                unbounditems: _AnyUnbound | None = None,
                _delay: float = 0.01,
            ) -> None: ...

        def put_nowait(self, obj: object, *, unbounditems: _AnyUnbound | None = None) -> None: ...
        if sys.version_info >= (3, 14):
            def get(self, block: bool = True, timeout: SupportsIndex | None = None, *, _delay: float = 0.01) -> object: ...
        else:
            def get(self, timeout: SupportsIndex | None = None, *, _delay: float = 0.01) -> object: ...

        def get_nowait(self) -> object: ...

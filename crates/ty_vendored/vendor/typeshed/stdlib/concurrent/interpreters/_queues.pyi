"""Cross-interpreter Queues High Level Module."""

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

    class QueueEmpty(QueueError, queue.Empty):
        """Raised from get_nowait() when the queue is empty.

        It is also raised from get() if it times out.
        """

    class QueueFull(QueueError, queue.Full):
        """Raised from put_nowait() when the queue is full.

        It is also raised from put() if it times out.
        """

    class ItemInterpreterDestroyed(QueueError, _crossinterp.ItemInterpreterDestroyed):
        """Raised from get() and get_nowait()."""

    UNBOUND: Final[UnboundItem]

    def create(maxsize: int = 0, *, unbounditems: _AnyUnbound = ...) -> Queue:
        """Return a new cross-interpreter queue.

        The queue may be used to pass data safely between interpreters.

        "unbounditems" sets the default for Queue.put(); see that method for
        supported values.  The default value is UNBOUND, which replaces
        the unbound item.
        """

    def list_all() -> list[Queue]:
        """Return a list of all open queues."""

    class Queue:
        """A cross-interpreter queue."""

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
            ) -> None:
                """Add the object to the queue.

                If "block" is true, this blocks while the queue is full.

                For most objects, the object received through Queue.get() will
                be a new one, equivalent to the original and not sharing any
                actual underlying data.  The notable exceptions include
                cross-interpreter types (like Queue) and memoryview, where the
                underlying data is actually shared.  Furthermore, some types
                can be sent through a queue more efficiently than others.  This
                group includes various immutable types like int, str, bytes, and
                tuple (if the items are likewise efficiently shareable).  See interpreters.is_shareable().

                "unbounditems" controls the behavior of Queue.get() for the given
                object if the current interpreter (calling put()) is later
                destroyed.

                If "unbounditems" is None (the default) then it uses the
                queue's default, set with create_queue(),
                which is usually UNBOUND.

                If "unbounditems" is UNBOUND_ERROR then get() will raise an
                ItemInterpreterDestroyed exception if the original interpreter
                has been destroyed.  This does not otherwise affect the queue;
                the next call to put() will work like normal, returning the next
                item in the queue.

                If "unbounditems" is UNBOUND_REMOVE then the item will be removed
                from the queue as soon as the original interpreter is destroyed.
                Be aware that this will introduce an imbalance between put()
                and get() calls.

                If "unbounditems" is UNBOUND then it is returned by get() in place
                of the unbound item.
                """
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
            def get(self, block: bool = True, timeout: SupportsIndex | None = None, *, _delay: float = 0.01) -> object:
                """Return the next object from the queue.

                If "block" is true, this blocks while the queue is empty.

                If the next item's original interpreter has been destroyed
                then the "next object" is determined by the value of the
                "unbounditems" argument to put().
                """
        else:
            def get(self, timeout: SupportsIndex | None = None, *, _delay: float = 0.01) -> object: ...

        def get_nowait(self) -> object:
            """Return the next object from the channel.

            If the queue is empty then raise QueueEmpty.  Otherwise this
            is the same as get().
            """

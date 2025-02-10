import sys
from collections.abc import Callable
from typing import Any, ClassVar, NamedTuple, type_check_only
from typing_extensions import TypeAlias

__all__ = ["scheduler"]

_ActionCallback: TypeAlias = Callable[..., Any]

if sys.version_info >= (3, 10):
    class Event(NamedTuple):
        time: float
        priority: Any
        sequence: int
        action: _ActionCallback
        argument: tuple[Any, ...]
        kwargs: dict[str, Any]

else:
    @type_check_only
    class _EventBase(NamedTuple):
        time: float
        priority: Any
        action: _ActionCallback
        argument: tuple[Any, ...]
        kwargs: dict[str, Any]

    class Event(_EventBase):
        __hash__: ClassVar[None]  # type: ignore[assignment]

class scheduler:
    timefunc: Callable[[], float]
    delayfunc: Callable[[float], object]

    def __init__(self, timefunc: Callable[[], float] = ..., delayfunc: Callable[[float], object] = ...) -> None: ...
    def enterabs(
        self, time: float, priority: Any, action: _ActionCallback, argument: tuple[Any, ...] = (), kwargs: dict[str, Any] = ...
    ) -> Event: ...
    def enter(
        self, delay: float, priority: Any, action: _ActionCallback, argument: tuple[Any, ...] = (), kwargs: dict[str, Any] = ...
    ) -> Event: ...
    def run(self, blocking: bool = True) -> float | None: ...
    def cancel(self, event: Event) -> None: ...
    def empty(self) -> bool: ...
    @property
    def queue(self) -> list[Event]: ...

import time
from collections.abc import Callable
from typing import Any, NamedTuple, TypeAlias

__all__ = ["scheduler"]

_ActionCallback: TypeAlias = Callable[..., Any]

class Event(NamedTuple):
    time: float
    priority: Any
    sequence: int
    action: _ActionCallback
    argument: tuple[Any, ...]
    kwargs: dict[str, Any]

class scheduler:
    timefunc: Callable[[], float]
    delayfunc: Callable[[float], object]

    def __init__(
        self, timefunc: Callable[[], float] = time.monotonic, delayfunc: Callable[[float], object] = time.sleep
    ) -> None: ...
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

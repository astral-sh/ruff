"""A generally useful event scheduler class.

Each instance of this class manages its own queue.
No multi-threading is implied; you are supposed to hack that
yourself, or use a single instance per application.

Each instance is parametrized with two functions, one that is
supposed to return the current time, one that is supposed to
implement a delay.  You can implement real-time scheduling by
substituting time and sleep from built-in module time, or you can
implement simulated time by writing your own functions.  This can
also be used to integrate scheduling with STDWIN events; the delay
function is allowed to modify the queue.  Time can be expressed as
integers or floating-point numbers, as long as it is consistent.

Events are specified by tuples (time, priority, action, argument, kwargs).
As in UNIX, lower priority numbers mean higher priority; in this
way the queue can be maintained as a priority queue.  Execution of the
event means calling the action function, passing it the argument
sequence in "argument" (remember that in Python, multiple function
arguments are be packed in a sequence) and keyword parameters in "kwargs".
The action function may be an instance method so it
has another way to reference private data (besides global variables).
"""

import sys
from collections.abc import Callable
from typing import Any, ClassVar, NamedTuple, type_check_only
from typing_extensions import TypeAlias

__all__ = ["scheduler"]

_ActionCallback: TypeAlias = Callable[..., Any]

if sys.version_info >= (3, 10):
    class Event(NamedTuple):
        """Event(time, priority, sequence, action, argument, kwargs)"""

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

    def __init__(self, timefunc: Callable[[], float] = ..., delayfunc: Callable[[float], object] = ...) -> None:
        """Initialize a new instance, passing the time and delay
        functions
        """

    def enterabs(
        self, time: float, priority: Any, action: _ActionCallback, argument: tuple[Any, ...] = (), kwargs: dict[str, Any] = ...
    ) -> Event:
        """Enter a new event in the queue at an absolute time.

        Returns an ID for the event which can be used to remove it,
        if necessary.

        """

    def enter(
        self, delay: float, priority: Any, action: _ActionCallback, argument: tuple[Any, ...] = (), kwargs: dict[str, Any] = ...
    ) -> Event:
        """A variant that specifies the time as a relative time.

        This is actually the more commonly used interface.

        """

    def run(self, blocking: bool = True) -> float | None:
        """Execute events until the queue is empty.
        If blocking is False executes the scheduled events due to
        expire soonest (if any) and then return the deadline of the
        next scheduled call in the scheduler.

        When there is a positive delay until the first event, the
        delay function is called and the event is left in the queue;
        otherwise, the event is removed from the queue and executed
        (its action function is called, passing it the argument).  If
        the delay function returns prematurely, it is simply
        restarted.

        It is legal for both the delay function and the action
        function to modify the queue or to raise an exception;
        exceptions are not caught but the scheduler's state remains
        well-defined so run() may be called again.

        A questionable hack is added to allow other threads to run:
        just after an event is executed, a delay of 0 is executed, to
        avoid monopolizing the CPU when other threads are also
        runnable.

        """

    def cancel(self, event: Event) -> None:
        """Remove an event from the queue.

        This must be presented the ID as returned by enter().
        If the event is not in the queue, this raises ValueError.

        """

    def empty(self) -> bool:
        """Check whether the queue is empty."""

    @property
    def queue(self) -> list[Event]:
        """An ordered list of upcoming events.

        Events are named tuples with fields for:
            time, priority, action, arguments, kwargs

        """

"""This module provides access to the garbage collector for reference cycles.

enable() -- Enable automatic garbage collection.
disable() -- Disable automatic garbage collection.
isenabled() -- Returns true if automatic collection is enabled.
collect() -- Do a full collection right now.
get_count() -- Return the current collection counts.
get_stats() -- Return list of dictionaries containing per-generation stats.
set_debug() -- Set debugging flags.
get_debug() -- Get debugging flags.
set_threshold() -- Set the collection thresholds.
get_threshold() -- Return the current collection thresholds.
get_objects() -- Return a list of all objects tracked by the collector.
is_tracked() -- Returns true if a given object is tracked.
is_finalized() -- Returns true if a given object has been already finalized.
get_referrers() -- Return the list of objects that refer to an object.
get_referents() -- Return the list of objects that an object refers to.
freeze() -- Freeze all tracked objects and ignore them for future collections.
unfreeze() -- Unfreeze all objects in the permanent generation.
get_freeze_count() -- Return the number of objects in the permanent generation.
"""

from collections.abc import Callable
from typing import Any, Final, Literal
from typing_extensions import TypeAlias

DEBUG_COLLECTABLE: Final = 2
DEBUG_LEAK: Final = 38
DEBUG_SAVEALL: Final = 32
DEBUG_STATS: Final = 1
DEBUG_UNCOLLECTABLE: Final = 4

_CallbackType: TypeAlias = Callable[[Literal["start", "stop"], dict[str, int]], object]

callbacks: list[_CallbackType]
garbage: list[Any]

def collect(generation: int = 2) -> int:
    """Run the garbage collector.

    With no arguments, run a full collection.  The optional argument
    may be an integer specifying which generation to collect.  A ValueError
    is raised if the generation number is invalid.

    The number of unreachable objects is returned.
    """

def disable() -> None:
    """Disable automatic garbage collection."""

def enable() -> None:
    """Enable automatic garbage collection."""

def get_count() -> tuple[int, int, int]:
    """Return a three-tuple of the current collection counts."""

def get_debug() -> int:
    """Get the garbage collection debugging flags."""

def get_objects(generation: int | None = None) -> list[Any]:
    """Return a list of objects tracked by the collector (excluding the list returned).

      generation
        Generation to extract the objects from.

    If generation is not None, return only the objects tracked by the collector
    that are in that generation.
    """

def freeze() -> None:
    """Freeze all current tracked objects and ignore them for future collections.

    This can be used before a POSIX fork() call to make the gc copy-on-write friendly.
    Note: collection before a POSIX fork() call may free pages for future allocation
    which can cause copy-on-write.
    """

def unfreeze() -> None:
    """Unfreeze all objects in the permanent generation.

    Put all objects in the permanent generation back into oldest generation.
    """

def get_freeze_count() -> int:
    """Return the number of objects in the permanent generation."""

def get_referents(*objs: Any) -> list[Any]:
    """Return the list of objects that are directly referred to by 'objs'."""

def get_referrers(*objs: Any) -> list[Any]:
    """Return the list of objects that directly refer to any of 'objs'."""

def get_stats() -> list[dict[str, Any]]:
    """Return a list of dictionaries containing per-generation statistics."""

def get_threshold() -> tuple[int, int, int]:
    """Return the current collection thresholds."""

def is_tracked(obj: Any, /) -> bool:
    """Returns true if the object is tracked by the garbage collector.

    Simple atomic objects will return false.
    """

def is_finalized(obj: Any, /) -> bool:
    """Returns true if the object has been already finalized by the GC."""

def isenabled() -> bool:
    """Returns true if automatic garbage collection is enabled."""

def set_debug(flags: int, /) -> None:
    """Set the garbage collection debugging flags.

      flags
        An integer that can have the following bits turned on:
          DEBUG_STATS - Print statistics during collection.
          DEBUG_COLLECTABLE - Print collectable objects found.
          DEBUG_UNCOLLECTABLE - Print unreachable but uncollectable objects
            found.
          DEBUG_SAVEALL - Save objects to gc.garbage rather than freeing them.
          DEBUG_LEAK - Debug leaking programs (everything but STATS).

    Debugging information is written to sys.stderr.
    """

def set_threshold(threshold0: int, threshold1: int = ..., threshold2: int = ..., /) -> None:
    """set_threshold(threshold0, [threshold1, [threshold2]])
    Set the collection thresholds (the collection frequency).

    Setting 'threshold0' to zero disables collection.
    """

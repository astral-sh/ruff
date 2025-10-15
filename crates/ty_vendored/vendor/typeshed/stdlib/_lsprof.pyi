"""Fast profiler"""

import sys
from _typeshed import structseq
from collections.abc import Callable
from types import CodeType
from typing import Any, Final, final
from typing_extensions import disjoint_base

@disjoint_base
class Profiler:
    """Build a profiler object using the specified timer function.

    The default timer is a fast built-in one based on real time.
    For custom timer functions returning integers, 'timeunit' can
    be a float specifying a scale (that is, how long each integer unit
    is, in seconds).
    """

    def __init__(
        self, timer: Callable[[], float] | None = None, timeunit: float = 0.0, subcalls: bool = True, builtins: bool = True
    ) -> None: ...
    def getstats(self) -> list[profiler_entry]:
        """list of profiler_entry objects.

        getstats() -> list of profiler_entry objects

        Return all information collected by the profiler.
        Each profiler_entry is a tuple-like object with the
        following attributes:

            code          code object
            callcount     how many times this was called
            reccallcount  how many times called recursively
            totaltime     total time in this entry
            inlinetime    inline time in this entry (not in subcalls)
            calls         details of the calls

        The calls attribute is either None or a list of
        profiler_subentry objects:

            code          called code object
            callcount     how many times this is called
            reccallcount  how many times this is called recursively
            totaltime     total time spent in this call
            inlinetime    inline time (not in further subcalls)
        """

    def enable(self, subcalls: bool = True, builtins: bool = True) -> None:
        """Start collecting profiling information.

        subcalls
          If True, also records for each function
          statistics separated according to its current caller.
        builtins
          If True, records the time spent in
          built-in functions separately from their caller.
        """

    def disable(self) -> None:
        """Stop collecting profiling information."""

    def clear(self) -> None:
        """Clear all profiling information collected so far."""

@final
class profiler_entry(structseq[Any], tuple[CodeType | str, int, int, float, float, list[profiler_subentry]]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("code", "callcount", "reccallcount", "totaltime", "inlinetime", "calls")
    code: CodeType | str
    callcount: int
    reccallcount: int
    totaltime: float
    inlinetime: float
    calls: list[profiler_subentry]

@final
class profiler_subentry(structseq[Any], tuple[CodeType | str, int, int, float, float]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("code", "callcount", "reccallcount", "totaltime", "inlinetime")
    code: CodeType | str
    callcount: int
    reccallcount: int
    totaltime: float
    inlinetime: float

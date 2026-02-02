import sys
from _tracemalloc import *
from collections.abc import Sequence
from typing import Any, SupportsIndex, overload
from typing_extensions import TypeAlias

def get_object_traceback(obj: object) -> Traceback | None:
    """
    Get the traceback where the Python object *obj* was allocated.
    Return a Traceback instance.

    Return None if the tracemalloc module is not tracing memory allocations or
    did not trace the allocation of the object.
    """

def take_snapshot() -> Snapshot:
    """
    Take a snapshot of traces of memory blocks allocated by Python.
    """

class BaseFilter:
    inclusive: bool
    def __init__(self, inclusive: bool) -> None: ...

class DomainFilter(BaseFilter):
    @property
    def domain(self) -> int: ...
    def __init__(self, inclusive: bool, domain: int) -> None: ...

class Filter(BaseFilter):
    domain: int | None
    lineno: int | None
    @property
    def filename_pattern(self) -> str: ...
    all_frames: bool
    def __init__(
        self,
        inclusive: bool,
        filename_pattern: str,
        lineno: int | None = None,
        all_frames: bool = False,
        domain: int | None = None,
    ) -> None: ...

class Statistic:
    """
    Statistic difference on memory allocations between two Snapshot instance.
    """

    __slots__ = ("traceback", "size", "count")
    count: int
    size: int
    traceback: Traceback
    def __init__(self, traceback: Traceback, size: int, count: int) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class StatisticDiff:
    """
    Statistic difference on memory allocations between an old and a new
    Snapshot instance.
    """

    __slots__ = ("traceback", "size", "size_diff", "count", "count_diff")
    count: int
    count_diff: int
    size: int
    size_diff: int
    traceback: Traceback
    def __init__(self, traceback: Traceback, size: int, size_diff: int, count: int, count_diff: int) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

_FrameTuple: TypeAlias = tuple[str, int]

class Frame:
    """
    Frame of a traceback.
    """

    __slots__ = ("_frame",)
    @property
    def filename(self) -> str: ...
    @property
    def lineno(self) -> int: ...
    def __init__(self, frame: _FrameTuple) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    def __lt__(self, other: Frame) -> bool: ...
    if sys.version_info >= (3, 11):
        def __gt__(self, other: Frame) -> bool:
            """Return a > b.  Computed by @total_ordering from (not a < b) and (a != b)."""

        def __ge__(self, other: Frame) -> bool:
            """Return a >= b.  Computed by @total_ordering from (not a < b)."""

        def __le__(self, other: Frame) -> bool:
            """Return a <= b.  Computed by @total_ordering from (a < b) or (a == b)."""
    else:
        def __gt__(self, other: Frame, NotImplemented: Any = ...) -> bool:
            """Return a > b.  Computed by @total_ordering from (not a < b) and (a != b)."""

        def __ge__(self, other: Frame, NotImplemented: Any = ...) -> bool:
            """Return a >= b.  Computed by @total_ordering from (not a < b)."""

        def __le__(self, other: Frame, NotImplemented: Any = ...) -> bool:
            """Return a <= b.  Computed by @total_ordering from (a < b) or (a == b)."""

_TraceTuple: TypeAlias = tuple[int, int, Sequence[_FrameTuple], int | None] | tuple[int, int, Sequence[_FrameTuple]]

class Trace:
    """
    Trace of a memory block.
    """

    __slots__ = ("_trace",)
    @property
    def domain(self) -> int: ...
    @property
    def size(self) -> int: ...
    @property
    def traceback(self) -> Traceback: ...
    def __init__(self, trace: _TraceTuple) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class Traceback(Sequence[Frame]):
    """
    Sequence of Frame instances sorted from the oldest frame
    to the most recent frame.
    """

    __slots__ = ("_frames", "_total_nframe")
    @property
    def total_nframe(self) -> int | None: ...
    def __init__(self, frames: Sequence[_FrameTuple], total_nframe: int | None = None) -> None: ...
    def format(self, limit: int | None = None, most_recent_first: bool = False) -> list[str]: ...
    @overload
    def __getitem__(self, index: SupportsIndex) -> Frame: ...
    @overload
    def __getitem__(self, index: slice) -> Sequence[Frame]: ...
    def __contains__(self, frame: Frame) -> bool: ...  # type: ignore[override]
    def __len__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    def __lt__(self, other: Traceback) -> bool: ...
    if sys.version_info >= (3, 11):
        def __gt__(self, other: Traceback) -> bool:
            """Return a > b.  Computed by @total_ordering from (not a < b) and (a != b)."""

        def __ge__(self, other: Traceback) -> bool:
            """Return a >= b.  Computed by @total_ordering from (not a < b)."""

        def __le__(self, other: Traceback) -> bool:
            """Return a <= b.  Computed by @total_ordering from (a < b) or (a == b)."""
    else:
        def __gt__(self, other: Traceback, NotImplemented: Any = ...) -> bool:
            """Return a > b.  Computed by @total_ordering from (not a < b) and (a != b)."""

        def __ge__(self, other: Traceback, NotImplemented: Any = ...) -> bool:
            """Return a >= b.  Computed by @total_ordering from (not a < b)."""

        def __le__(self, other: Traceback, NotImplemented: Any = ...) -> bool:
            """Return a <= b.  Computed by @total_ordering from (a < b) or (a == b)."""

class Snapshot:
    """
    Snapshot of traces of memory blocks allocated by Python.
    """

    def __init__(self, traces: Sequence[_TraceTuple], traceback_limit: int) -> None: ...
    def compare_to(self, old_snapshot: Snapshot, key_type: str, cumulative: bool = False) -> list[StatisticDiff]:
        """
        Compute the differences with an old snapshot old_snapshot. Get
        statistics as a sorted list of StatisticDiff instances, grouped by
        group_by.
        """

    def dump(self, filename: str) -> None:
        """
        Write the snapshot into a file.
        """

    def filter_traces(self, filters: Sequence[DomainFilter | Filter]) -> Snapshot:
        """
        Create a new Snapshot instance with a filtered traces sequence, filters
        is a list of Filter or DomainFilter instances.  If filters is an empty
        list, return a new Snapshot instance with a copy of the traces.
        """

    @staticmethod
    def load(filename: str) -> Snapshot:
        """
        Load a snapshot from a file.
        """

    def statistics(self, key_type: str, cumulative: bool = False) -> list[Statistic]:
        """
        Group statistics by key_type. Return a sorted list of Statistic
        instances.
        """
    traceback_limit: int
    traces: Sequence[Trace]

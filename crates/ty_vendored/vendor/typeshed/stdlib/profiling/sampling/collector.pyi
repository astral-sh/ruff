from _typeshed import StrOrBytesPath
from abc import ABC, abstractmethod
from collections.abc import Sequence
from typing import ClassVar, TypeAlias

from _remote_debugging import AwaitedInfo, FrameInfo, InterpreterInfo, LocationInfo

_Location: TypeAlias = int | tuple[int, int, int, int] | LocationInfo | None
_Frame: TypeAlias = FrameInfo | tuple[str, _Location, str, int | None]
_Timestamps: TypeAlias = Sequence[int] | None

def normalize_location(location: _Location) -> tuple[int, int, int, int]:
    """Normalize location to a 4-tuple format.

Args:
    location: tuple (lineno, end_lineno, col_offset, end_col_offset),
        an integer line number, or None

Returns:
    tuple: (lineno, end_lineno, col_offset, end_col_offset)
"""
def extract_lineno(location: _Location) -> int:
    """Extract lineno from location.

Args:
    location: tuple (lineno, end_lineno, col_offset, end_col_offset),
        an integer line number, or None

Returns:
    int: The line number (0 for synthetic frames)
"""
def filter_internal_frames(frames: Sequence[_Frame]) -> list[_Frame]: ...
def iter_async_frames(awaited_info_list: Sequence[AwaitedInfo]) -> object: ...

class Collector(ABC):
    aggregating: ClassVar[bool]  # undocumented
    @abstractmethod
    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None:
        """Collect profiling data from stack frames.

Args:
    stack_frames: List of InterpreterInfo objects
    timestamps_us: Optional list of timestamps in microseconds. If provided
        (from binary replay with RLE batching), use these instead of current
        time. If None, collectors should use time.monotonic() or similar.
        The list may contain multiple timestamps when samples are batched
        together (same stack, different times).
"""
    def collect_failed_sample(self) -> None:
        """Collect data about a failed sample attempt."""
    @abstractmethod
    def export(self, filename: StrOrBytesPath) -> None:
        """Export collected data to a file."""

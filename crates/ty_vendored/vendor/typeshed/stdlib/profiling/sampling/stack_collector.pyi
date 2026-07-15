from _typeshed import StrOrBytesPath
from abc import ABCMeta
from collections.abc import Sequence

from _remote_debugging import AwaitedInfo, InterpreterInfo

from .collector import Collector, _Frame, _Timestamps

class StackTraceCollector(Collector, metaclass=ABCMeta):
    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False) -> None: ...
    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None: ...
    def process_frames(self, frames: Sequence[_Frame], thread_id: int, weight: int = 1) -> None: ...

class CollapsedStackCollector(StackTraceCollector):
    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False) -> None: ...
    def process_frames(self, frames: Sequence[_Frame], thread_id: int, weight: int = 1) -> None: ...
    def export(self, filename: StrOrBytesPath) -> None: ...

class FlamegraphCollector(StackTraceCollector):
    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False) -> None: ...
    def collect(self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None) -> None:
        """Override to track thread status statistics before processing frames."""

    def set_stats(
        self,
        sample_interval_usec: int,
        duration_sec: float,
        sample_rate: float,
        error_rate: float | None = None,
        missed_samples: float | None = None,
        mode: int | None = None,
    ) -> None:
        """Set profiling statistics to include in flamegraph data."""

    def export(self, filename: StrOrBytesPath) -> None: ...
    def process_frames(self, frames: Sequence[_Frame], thread_id: int, weight: int = 1) -> None:
        """Process stack frames into flamegraph tree structure.

        Args:
            frames: List of (filename, location, funcname, opcode) tuples in
                    leaf-to-root order. location is (lineno, end_lineno, col_offset, end_col_offset).
                    opcode is None if not gathered.
            thread_id: Thread ID for this stack trace
            weight: Number of samples this stack represents (for batched RLE)
        """

class DiffFlamegraphCollector(FlamegraphCollector):
    """Differential flamegraph collector that compares against a baseline binary profile."""

    def __init__(self, sample_interval_usec: int, *, baseline_binary_path: StrOrBytesPath, skip_idle: bool = False) -> None: ...

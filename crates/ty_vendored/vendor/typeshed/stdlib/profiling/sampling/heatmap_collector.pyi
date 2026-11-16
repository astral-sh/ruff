"""Heatmap collector for Python profiling with line-level execution heat visualization."""

from _typeshed import StrOrBytesPath
from collections.abc import Sequence

from _remote_debugging import AwaitedInfo, InterpreterInfo

from .collector import Collector, _Frame, _Timestamps

class HeatmapCollector(Collector):
    """Collector that generates coverage.py-style heatmap HTML output with line intensity.

    This collector creates detailed HTML reports showing which lines of code
    were executed most frequently during profiling, similar to coverage.py
    but showing execution "heat" rather than just coverage.
    """

    FILE_INDEX_FORMAT: str
    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False) -> None:
        """Initialize the heatmap collector with data structures for analysis."""

    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None: ...
    def export(self, output_path: StrOrBytesPath) -> None:
        """Export heatmap data as HTML files in a directory.

        Args:
            output_path: Path where to create the heatmap output directory
        """

    def process_frames(self, frames: Sequence[_Frame], thread_id: int, weight: int = 1) -> None:
        """Process stack frames and count samples per line.

        Args:
            frames: List of (filename, location, funcname, opcode) tuples in
                    leaf-to-root order. location is (lineno, end_lineno, col_offset, end_col_offset).
                    opcode is None if not gathered.
            thread_id: Thread ID for this stack trace
            weight: Number of samples this stack represents (for batched RLE)
        """

    def set_stats(
        self,
        sample_interval_usec: int,
        duration_sec: float,
        sample_rate: float,
        error_rate: float | None = None,
        missed_samples: float | None = None,
        **kwargs: object,
    ) -> None:
        """Set profiling statistics to include in heatmap output.

        Args:
            sample_interval_usec: Sampling interval in microseconds
            duration_sec: Total profiling duration in seconds
            sample_rate: Effective sampling rate
            error_rate: Optional error rate during profiling
            missed_samples: Optional percentage of missed samples
            **kwargs: Additional statistics to include
        """

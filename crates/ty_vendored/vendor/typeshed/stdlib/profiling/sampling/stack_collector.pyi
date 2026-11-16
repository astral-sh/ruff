from _typeshed import StrOrBytesPath, SupportsGet, SupportsKeysAndGet
from abc import ABCMeta
from collections import Counter
from collections.abc import Sequence
from typing import Literal, TypeAlias, TypedDict, type_check_only
from typing_extensions import Required

from _remote_debugging import AwaitedInfo, InterpreterInfo

from .collector import Collector, _Frame, _Location, _Timestamps

# type check only
# TODO: move to binary_collector.pyi when that is added
_CaptureFeatureKeys: TypeAlias = Literal["all_threads", "native", "gc", "opcodes", "blocking"]

class StackTraceCollector(Collector, metaclass=ABCMeta):
    sample_interval_usec: int
    skip_idle: bool

    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False) -> None: ...
    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None: ...
    def process_frames(self, frames: Sequence[_Frame], thread_id: int, weight: int = 1) -> None: ...

class CollapsedStackCollector(StackTraceCollector):
    stack_counter: Counter[tuple[tuple[str, _Location, str], int]]
    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False) -> None: ...
    def process_frames(self, frames: Sequence[_Frame], thread_id: int, weight: int = 1) -> None: ...
    def export(self, filename: StrOrBytesPath) -> None: ...

@type_check_only
class _FlamegraphCollectorStats(TypedDict, total=False):
    sample_interval_usec: Required[int]
    duration_sec: float
    sample_rate: float
    error_rate: float | None
    missed_samples: float | None
    mode: int | None

@type_check_only
class _ThreadStatusCounts(TypedDict):
    has_gil: int
    on_cpu: int
    gil_requested: int
    unknown: int
    has_exception: int
    total: int

class FlamegraphCollector(StackTraceCollector):
    stats: _FlamegraphCollectorStats
    thread_status_counts: _ThreadStatusCounts
    samples_with_gc_frames: int
    per_thread_stats: dict[int, _ThreadStatusCounts]

    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False) -> None: ...
    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None: ...
    def set_stats(
        self,
        sample_interval_usec: int,
        duration_sec: float,
        sample_rate: float,
        error_rate: float | None = None,
        missed_samples: float | None = None,
        mode: int | None = None,
    ) -> None: ...
    def set_replay_stats(self, info: SupportsGet[str, float | None]) -> None: ...
    def set_mode(self, mode: int | None) -> None: ...
    def export(self, filename: StrOrBytesPath) -> None: ...
    def process_frames(self, frames: Sequence[_Frame], thread_id: int, weight: int = 1) -> None: ...

class DiffFlamegraphCollector(FlamegraphCollector):
    baseline_binary_path: StrOrBytesPath
    mode: int | None
    capture_config: SupportsKeysAndGet[_CaptureFeatureKeys, bool] | None

    def __init__(
        self,
        sample_interval_usec: int,
        *,
        baseline_binary_path: StrOrBytesPath,
        skip_idle: bool = False,
        mode: int | None = None,
        capture_config: SupportsKeysAndGet[_CaptureFeatureKeys, bool] | None = None,
    ) -> None: ...

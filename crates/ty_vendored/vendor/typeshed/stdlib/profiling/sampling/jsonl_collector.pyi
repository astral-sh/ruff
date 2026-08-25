from _typeshed import StrOrBytesPath
from collections.abc import Sequence

from _remote_debugging import AwaitedInfo, InterpreterInfo

from .collector import _Frame, _Timestamps
from .stack_collector import StackTraceCollector

class JsonlCollector(StackTraceCollector):
    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False, mode: int | None = None) -> None: ...
    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None: ...
    def export(self, filename: StrOrBytesPath) -> None: ...
    def process_frames(self, frames: Sequence[_Frame], _thread_id: int, weight: int = 1) -> None: ...

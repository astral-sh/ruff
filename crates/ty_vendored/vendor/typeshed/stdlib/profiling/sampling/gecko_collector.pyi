from _typeshed import StrOrBytesPath
from collections.abc import Sequence

from _remote_debugging import AwaitedInfo, InterpreterInfo

from .collector import Collector, _Timestamps

class GeckoCollector(Collector):
    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False, opcodes: bool = False) -> None: ...
    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None: ...
    def export(self, filename: StrOrBytesPath) -> None: ...

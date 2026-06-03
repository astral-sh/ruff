from _typeshed import StrOrBytesPath
from abc import ABC, abstractmethod
from collections.abc import Sequence
from typing import TypeAlias

from _remote_debugging import AwaitedInfo, FrameInfo, InterpreterInfo, LocationInfo

_Location: TypeAlias = int | tuple[int, int, int, int] | LocationInfo | None
_Frame: TypeAlias = FrameInfo | tuple[str, _Location, str, int | None]
_Timestamps: TypeAlias = Sequence[int] | None

def normalize_location(location: _Location) -> tuple[int, int, int, int]: ...
def extract_lineno(location: _Location) -> int: ...
def filter_internal_frames(frames: Sequence[_Frame]) -> list[_Frame]: ...
def iter_async_frames(awaited_info_list: Sequence[AwaitedInfo]) -> object: ...

class Collector(ABC):
    @abstractmethod
    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None: ...
    def collect_failed_sample(self) -> None: ...
    @abstractmethod
    def export(self, filename: StrOrBytesPath) -> None: ...

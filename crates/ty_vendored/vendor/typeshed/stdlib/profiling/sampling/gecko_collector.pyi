from _typeshed import Incomplete, StrOrBytesPath, StrPath
from collections.abc import Generator, Sequence
from tempfile import TemporaryDirectory
from typing import Any, ClassVar, Final, TypedDict, type_check_only

from _remote_debugging import AwaitedInfo, InterpreterInfo

from .collector import Collector, _Timestamps

@type_check_only
class _GeckoCategory(TypedDict):
    name: str
    color: str
    subcategories: list[str]

THREAD_STATUS_HAS_GIL: Final[int]
THREAD_STATUS_ON_CPU: Final[int]
THREAD_STATUS_UNKNOWN: Final[int]
THREAD_STATUS_GIL_REQUESTED: Final[int]
THREAD_STATUS_HAS_EXCEPTION: Final[int]
THREAD_STATUS_MAIN_THREAD: Final[int]

GECKO_CATEGORIES: Final[list[_GeckoCategory]]

CATEGORY_OTHER: Final = 0
CATEGORY_PYTHON: Final = 1
CATEGORY_NATIVE: Final = 2
CATEGORY_GC: Final = 3
CATEGORY_GIL: Final = 4
CATEGORY_CPU: Final = 5
CATEGORY_CODE_TYPE: Final = 6
CATEGORY_OPCODES: Final = 7
CATEGORY_EXCEPTION: Final = 8

DEFAULT_SUBCATEGORY: Final = 0

GECKO_FORMAT_VERSION: Final = 32
GECKO_PREPROCESSED_VERSION: Final = 57

RESOURCE_TYPE_LIBRARY: Final = 1

FRAME_ADDRESS_NONE: Final = -1
FRAME_INLINE_DEPTH_ROOT: Final = 0

PROCESS_TYPE_MAIN: Final = 0
STACKWALK_DISABLED: Final = 0

DEFAULT_SPILL_BUFFER_BYTES: Final[int]

class SpillColumn:
    path: str
    buffer: bytearray

    def __init__(self, directory: StrPath, basename: StrPath, *, buffer_bytes: int | None = None) -> None: ...
    # "value" accepts the same types as json.JSONEncoder.encode()
    def append(self, value: Any) -> None: ...
    def flush(self) -> None: ...
    def iter_tokens(self) -> Generator[str]: ...

class GeckoThreadSpill:
    sample_count: int
    marker_count: int
    def __init__(self, directory: StrPath, tid: int) -> None: ...
    def append_sample(self, stack_index: int, time_ms: float) -> None: ...
    def append_marker(
        self, name_idx: int, start_time: float, end_time: float, phase: int, category: int, data: dict[str, Any]
    ) -> None: ...
    def prepare_read(self) -> None: ...

class GeckoCollector(Collector):
    aggregating: ClassVar[bool]

    sample_interval_usec: int
    skip_idle: bool
    opcodes_enabled: bool
    start_time: float

    global_strings: list[str]
    global_string_map: dict[str, int]

    threads: dict[int, dict[str, Any]]
    spill_dir: TemporaryDirectory[str] | None
    exported: bool

    libs: list[Incomplete]

    sample_count: int
    last_sample_time: float
    interval: float

    has_gil_start: dict[Incomplete, Incomplete]
    no_gil_start: dict[Incomplete, Incomplete]
    on_cpu_start: dict[Incomplete, Incomplete]
    off_cpu_start: dict[Incomplete, Incomplete]
    python_code_start: dict[Incomplete, Incomplete]
    native_code_start: dict[Incomplete, Incomplete]
    gil_wait_start: dict[Incomplete, Incomplete]
    exception_start: dict[Incomplete, Incomplete]
    no_exception_start: dict[Incomplete, Incomplete]

    gc_start_per_thread: dict[int, float]

    initialized_threads: set[Incomplete]

    opcode_state: dict[int, tuple[Incomplete, int, int, str, str, float]]

    def __init__(self, sample_interval_usec: int, *, skip_idle: bool = False, opcodes: bool = False) -> None: ...
    def collect(
        self, stack_frames: Sequence[InterpreterInfo] | Sequence[AwaitedInfo], timestamps_us: _Timestamps = None
    ) -> None: ...
    def export(self, filename: StrOrBytesPath) -> None: ...

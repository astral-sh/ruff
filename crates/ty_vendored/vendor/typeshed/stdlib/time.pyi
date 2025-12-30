import sys
from _typeshed import structseq
from typing import Any, Final, Literal, Protocol, final, type_check_only
from typing_extensions import TypeAlias

_TimeTuple: TypeAlias = tuple[int, int, int, int, int, int, int, int, int]

altzone: int
daylight: int
timezone: int
tzname: tuple[str, str]

if sys.platform == "linux":
    CLOCK_BOOTTIME: Final[int]
if sys.platform != "linux" and sys.platform != "win32" and sys.platform != "darwin":
    CLOCK_PROF: Final[int]  # FreeBSD, NetBSD, OpenBSD
    CLOCK_UPTIME: Final[int]  # FreeBSD, OpenBSD

if sys.platform != "win32":
    CLOCK_MONOTONIC: Final[int]
    CLOCK_MONOTONIC_RAW: Final[int]
    CLOCK_PROCESS_CPUTIME_ID: Final[int]
    CLOCK_REALTIME: Final[int]
    CLOCK_THREAD_CPUTIME_ID: Final[int]
    if sys.platform != "linux" and sys.platform != "darwin":
        CLOCK_HIGHRES: Final[int]  # Solaris only

if sys.platform == "darwin":
    CLOCK_UPTIME_RAW: Final[int]
    if sys.version_info >= (3, 13):
        CLOCK_UPTIME_RAW_APPROX: Final[int]
        CLOCK_MONOTONIC_RAW_APPROX: Final[int]

if sys.platform == "linux":
    CLOCK_TAI: Final[int]

# Constructor takes an iterable of any type, of length between 9 and 11 elements.
# However, it always *behaves* like a tuple of 9 elements,
# even if an iterable with length >9 is passed.
# https://github.com/python/typeshed/pull/6560#discussion_r767162532
@final
class struct_time(structseq[Any | int], _TimeTuple):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("tm_year", "tm_mon", "tm_mday", "tm_hour", "tm_min", "tm_sec", "tm_wday", "tm_yday", "tm_isdst")

    @property
    def tm_year(self) -> int: ...
    @property
    def tm_mon(self) -> int: ...
    @property
    def tm_mday(self) -> int: ...
    @property
    def tm_hour(self) -> int: ...
    @property
    def tm_min(self) -> int: ...
    @property
    def tm_sec(self) -> int: ...
    @property
    def tm_wday(self) -> int: ...
    @property
    def tm_yday(self) -> int: ...
    @property
    def tm_isdst(self) -> int: ...
    # These final two properties only exist if a 10- or 11-item sequence was passed to the constructor.
    @property
    def tm_zone(self) -> str: ...
    @property
    def tm_gmtoff(self) -> int: ...

def asctime(time_tuple: _TimeTuple | struct_time = ..., /) -> str: ...
def ctime(seconds: float | None = None, /) -> str: ...
def gmtime(seconds: float | None = None, /) -> struct_time: ...
def localtime(seconds: float | None = None, /) -> struct_time: ...
def mktime(time_tuple: _TimeTuple | struct_time, /) -> float: ...
def sleep(seconds: float, /) -> None: ...
def strftime(format: str, time_tuple: _TimeTuple | struct_time = ..., /) -> str: ...
def strptime(data_string: str, format: str = "%a %b %d %H:%M:%S %Y", /) -> struct_time: ...
def time() -> float: ...

if sys.platform != "win32":
    def tzset() -> None: ...  # Unix only

@type_check_only
class _ClockInfo(Protocol):
    adjustable: bool
    implementation: str
    monotonic: bool
    resolution: float

def get_clock_info(name: Literal["monotonic", "perf_counter", "process_time", "time", "thread_time"], /) -> _ClockInfo: ...
def monotonic() -> float: ...
def perf_counter() -> float: ...
def process_time() -> float: ...

if sys.platform != "win32":
    def clock_getres(clk_id: int, /) -> float: ...  # Unix only
    def clock_gettime(clk_id: int, /) -> float: ...  # Unix only
    def clock_settime(clk_id: int, time: float, /) -> None: ...  # Unix only

if sys.platform != "win32":
    def clock_gettime_ns(clk_id: int, /) -> int: ...
    def clock_settime_ns(clock_id: int, time: int, /) -> int: ...

if sys.platform == "linux":
    def pthread_getcpuclockid(thread_id: int, /) -> int: ...

def monotonic_ns() -> int: ...
def perf_counter_ns() -> int: ...
def process_time_ns() -> int: ...
def time_ns() -> int: ...
def thread_time() -> float: ...
def thread_time_ns() -> int: ...

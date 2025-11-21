"""This module provides various functions to manipulate time values.

There are two standard representations of time.  One is the number
of seconds since the Epoch, in UTC (a.k.a. GMT).  It may be an integer
or a floating-point number (to represent fractions of seconds).
The epoch is the point where the time starts, the return value of time.gmtime(0).
It is January 1, 1970, 00:00:00 (UTC) on all platforms.

The other representation is a tuple of 9 integers giving local time.
The tuple items are:
  year (including century, e.g. 1998)
  month (1-12)
  day (1-31)
  hours (0-23)
  minutes (0-59)
  seconds (0-59)
  weekday (0-6, Monday is 0)
  Julian day (day in the year, 1-366)
  DST (Daylight Savings Time) flag (-1, 0 or 1)
If the DST flag is 0, the time is given in the regular time zone;
if it is 1, the time is given in the DST time zone;
if it is -1, mktime() should guess based on the date and time.
"""

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
    """The time value as returned by gmtime(), localtime(), and strptime(), and
    accepted by asctime(), mktime() and strftime().  May be considered as a
    sequence of 9 integers.

    Note that several fields' values are not the same as those defined by
    the C language standard for struct tm.  For example, the value of the
    field tm_year is the actual year, not year - 1900.  See individual
    fields' descriptions for details.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = ("tm_year", "tm_mon", "tm_mday", "tm_hour", "tm_min", "tm_sec", "tm_wday", "tm_yday", "tm_isdst")

    @property
    def tm_year(self) -> int:
        """year, for example, 1993"""

    @property
    def tm_mon(self) -> int:
        """month of year, range [1, 12]"""

    @property
    def tm_mday(self) -> int:
        """day of month, range [1, 31]"""

    @property
    def tm_hour(self) -> int:
        """hours, range [0, 23]"""

    @property
    def tm_min(self) -> int:
        """minutes, range [0, 59]"""

    @property
    def tm_sec(self) -> int:
        """seconds, range [0, 61])"""

    @property
    def tm_wday(self) -> int:
        """day of week, range [0, 6], Monday is 0"""

    @property
    def tm_yday(self) -> int:
        """day of year, range [1, 366]"""

    @property
    def tm_isdst(self) -> int:
        """1 if summer time is in effect, 0 if not, and -1 if unknown"""
    # These final two properties only exist if a 10- or 11-item sequence was passed to the constructor.
    @property
    def tm_zone(self) -> str:
        """abbreviation of timezone name"""

    @property
    def tm_gmtoff(self) -> int:
        """offset from UTC in seconds"""

def asctime(time_tuple: _TimeTuple | struct_time = ..., /) -> str:
    """asctime([tuple]) -> string

    Convert a time tuple to a string, e.g. 'Sat Jun 06 16:26:11 1998'.
    When the time tuple is not present, current time as returned by localtime()
    is used.
    """

def ctime(seconds: float | None = None, /) -> str:
    """ctime(seconds) -> string

    Convert a time in seconds since the Epoch to a string in local time.
    This is equivalent to asctime(localtime(seconds)). When the time tuple is
    not present, current time as returned by localtime() is used.
    """

def gmtime(seconds: float | None = None, /) -> struct_time:
    """gmtime([seconds]) -> (tm_year, tm_mon, tm_mday, tm_hour, tm_min,
                           tm_sec, tm_wday, tm_yday, tm_isdst)

    Convert seconds since the Epoch to a time tuple expressing UTC (a.k.a.
    GMT).  When 'seconds' is not passed in, convert the current time instead.

    If the platform supports the tm_gmtoff and tm_zone, they are available as
    attributes only.
    """

def localtime(seconds: float | None = None, /) -> struct_time:
    """localtime([seconds]) -> (tm_year,tm_mon,tm_mday,tm_hour,tm_min,
                              tm_sec,tm_wday,tm_yday,tm_isdst)

    Convert seconds since the Epoch to a time tuple expressing local time.
    When 'seconds' is not passed in, convert the current time instead.
    """

def mktime(time_tuple: _TimeTuple | struct_time, /) -> float:
    """mktime(tuple) -> floating-point number

    Convert a time tuple in local time to seconds since the Epoch.
    Note that mktime(gmtime(0)) will not generally return zero for most
    time zones; instead the returned value will either be equal to that
    of the timezone or altzone attributes on the time module.
    """

def sleep(seconds: float, /) -> None:
    """sleep(seconds)

    Delay execution for a given number of seconds.  The argument may be
    a floating-point number for subsecond precision.
    """

def strftime(format: str, time_tuple: _TimeTuple | struct_time = ..., /) -> str:
    """strftime(format[, tuple]) -> string

    Convert a time tuple to a string according to a format specification.
    See the library reference manual for formatting codes. When the time tuple
    is not present, current time as returned by localtime() is used.

    Commonly used format codes:

    %Y  Year with century as a decimal number.
    %m  Month as a decimal number [01,12].
    %d  Day of the month as a decimal number [01,31].
    %H  Hour (24-hour clock) as a decimal number [00,23].
    %M  Minute as a decimal number [00,59].
    %S  Second as a decimal number [00,61].
    %z  Time zone offset from UTC.
    %a  Locale's abbreviated weekday name.
    %A  Locale's full weekday name.
    %b  Locale's abbreviated month name.
    %B  Locale's full month name.
    %c  Locale's appropriate date and time representation.
    %I  Hour (12-hour clock) as a decimal number [01,12].
    %p  Locale's equivalent of either AM or PM.

    Other codes may be available on your platform.  See documentation for
    the C library strftime function.
    """

def strptime(data_string: str, format: str = "%a %b %d %H:%M:%S %Y", /) -> struct_time:
    """strptime(string, format) -> struct_time

    Parse a string to a time tuple according to a format specification.
    See the library reference manual for formatting codes (same as
    strftime()).

    Commonly used format codes:

    %Y  Year with century as a decimal number.
    %m  Month as a decimal number [01,12].
    %d  Day of the month as a decimal number [01,31].
    %H  Hour (24-hour clock) as a decimal number [00,23].
    %M  Minute as a decimal number [00,59].
    %S  Second as a decimal number [00,61].
    %z  Time zone offset from UTC.
    %a  Locale's abbreviated weekday name.
    %A  Locale's full weekday name.
    %b  Locale's abbreviated month name.
    %B  Locale's full month name.
    %c  Locale's appropriate date and time representation.
    %I  Hour (12-hour clock) as a decimal number [01,12].
    %p  Locale's equivalent of either AM or PM.

    Other codes may be available on your platform.  See documentation for
    the C library strftime function.
    """

def time() -> float:
    """time() -> floating-point number

    Return the current time in seconds since the Epoch.
    Fractions of a second may be present if the system clock provides them.
    """

if sys.platform != "win32":
    def tzset() -> None:  # Unix only
        """tzset()

        Initialize, or reinitialize, the local timezone to the value stored in
        os.environ['TZ']. The TZ environment variable should be specified in
        standard Unix timezone format as documented in the tzset man page
        (eg. 'US/Eastern', 'Europe/Amsterdam'). Unknown timezones will silently
        fall back to UTC. If the TZ environment variable is not set, the local
        timezone is set to the systems best guess of wallclock time.
        Changing the TZ environment variable without calling tzset *may* change
        the local timezone used by methods such as localtime, but this behaviour
        should not be relied on.
        """

@type_check_only
class _ClockInfo(Protocol):
    adjustable: bool
    implementation: str
    monotonic: bool
    resolution: float

def get_clock_info(name: Literal["monotonic", "perf_counter", "process_time", "time", "thread_time"], /) -> _ClockInfo:
    """get_clock_info(name: str) -> dict

    Get information of the specified clock.
    """

def monotonic() -> float:
    """monotonic() -> float

    Monotonic clock, cannot go backward.
    """

def perf_counter() -> float:
    """perf_counter() -> float

    Performance counter for benchmarking.
    """

def process_time() -> float:
    """process_time() -> float

    Process time for profiling: sum of the kernel and user-space CPU time.
    """

if sys.platform != "win32":
    def clock_getres(clk_id: int, /) -> float:  # Unix only
        """clock_getres(clk_id) -> floating-point number

        Return the resolution (precision) of the specified clock clk_id.
        """

    def clock_gettime(clk_id: int, /) -> float:  # Unix only
        """Return the time of the specified clock clk_id as a float."""

    def clock_settime(clk_id: int, time: float, /) -> None:  # Unix only
        """clock_settime(clk_id, time)

        Set the time of the specified clock clk_id.
        """

if sys.platform != "win32":
    def clock_gettime_ns(clk_id: int, /) -> int:
        """Return the time of the specified clock clk_id as nanoseconds (int)."""

    def clock_settime_ns(clock_id: int, time: int, /) -> int:
        """clock_settime_ns(clk_id, time)

        Set the time of the specified clock clk_id with nanoseconds.
        """

if sys.platform == "linux":
    def pthread_getcpuclockid(thread_id: int, /) -> int:
        """pthread_getcpuclockid(thread_id) -> int

        Return the clk_id of a thread's CPU time clock.
        """

def monotonic_ns() -> int:
    """monotonic_ns() -> int

    Monotonic clock, cannot go backward, as nanoseconds.
    """

def perf_counter_ns() -> int:
    """perf_counter_ns() -> int

    Performance counter for benchmarking as nanoseconds.
    """

def process_time_ns() -> int:
    """process_time() -> int

    Process time for profiling as nanoseconds:
    sum of the kernel and user-space CPU time.
    """

def time_ns() -> int:
    """time_ns() -> int

    Return the current time in nanoseconds since the Epoch.
    """

def thread_time() -> float:
    """thread_time() -> float

    Thread time for profiling: sum of the kernel and user-space CPU time.
    """

def thread_time_ns() -> int:
    """thread_time() -> int

    Thread time for profiling as nanoseconds:
    sum of the kernel and user-space CPU time.
    """

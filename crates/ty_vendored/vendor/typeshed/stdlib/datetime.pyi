import sys
from abc import abstractmethod
from time import struct_time
from typing import Any, ClassVar, Final, Generic, Literal, SupportsIndex, TypeAlias, TypeVar, final, overload, type_check_only
from typing_extensions import CapsuleType, Never, Self, deprecated, disjoint_base

if sys.version_info >= (3, 11):
    __all__ = ("date", "datetime", "time", "timedelta", "timezone", "tzinfo", "MINYEAR", "MAXYEAR", "UTC")
else:
    __all__ = ("date", "datetime", "time", "timedelta", "timezone", "tzinfo", "MINYEAR", "MAXYEAR")

MINYEAR: Final = 1
MAXYEAR: Final = 9999

class tzinfo:
    @abstractmethod
    def tzname(self, dt: datetime | None, /) -> str | None: ...
    @abstractmethod
    def utcoffset(self, dt: datetime | None, /) -> timedelta | None: ...
    @abstractmethod
    def dst(self, dt: datetime | None, /) -> timedelta | None: ...
    def fromutc(self, dt: datetime, /) -> datetime: ...

# Alias required to avoid name conflicts with date(time).tzinfo.
_TzInfo: TypeAlias = tzinfo

@final
class timezone(tzinfo):
    utc: ClassVar[timezone]
    min: ClassVar[timezone]
    max: ClassVar[timezone]
    def __new__(cls, offset: timedelta, name: str = ...) -> Self: ...
    def tzname(self, dt: datetime | None, /) -> str: ...
    def utcoffset(self, dt: datetime | None, /) -> timedelta: ...
    def dst(self, dt: datetime | None, /) -> None: ...
    def __hash__(self) -> int: ...
    def __eq__(self, value: object, /) -> bool: ...

if sys.version_info >= (3, 11):
    UTC: timezone

# This class calls itself datetime.IsoCalendarDate. It's neither
# NamedTuple nor structseq.
@final
@type_check_only
class _IsoCalendarDate(tuple[int, int, int]):
    @property
    def year(self) -> int: ...
    @property
    def week(self) -> int: ...
    @property
    def weekday(self) -> int: ...

@disjoint_base
class date:
    min: ClassVar[date]
    max: ClassVar[date]
    resolution: ClassVar[timedelta]
    def __new__(cls, year: SupportsIndex, month: SupportsIndex, day: SupportsIndex) -> Self: ...
    @classmethod
    def fromtimestamp(cls, timestamp: float, /) -> Self: ...
    @classmethod
    def today(cls) -> Self: ...
    @classmethod
    def fromordinal(cls, n: int, /) -> Self: ...

    if sys.version_info >= (3, 15):
        @classmethod
        def fromisoformat(cls, string: str, /) -> Self: ...
    else:
        @classmethod
        def fromisoformat(cls, date_string: str, /) -> Self: ...

    @classmethod
    def fromisocalendar(cls, year: int, week: int, day: int) -> Self: ...
    @property
    def year(self) -> int: ...
    @property
    def month(self) -> int: ...
    @property
    def day(self) -> int: ...
    def ctime(self) -> str: ...

    if sys.version_info >= (3, 14):
        if sys.version_info >= (3, 15):
            @classmethod
            def strptime(cls, string: str, format: str, /) -> Self: ...
        else:
            @classmethod
            def strptime(cls, date_string: str, format: str, /) -> Self: ...

    # On <3.12, the name of the parameter in the pure-Python implementation
    # didn't match the name in the C implementation,
    # meaning it is only *safe* to pass it as a keyword argument on 3.12+
    if sys.version_info >= (3, 12):
        def strftime(self, format: str) -> str: ...
    else:
        def strftime(self, format: str, /) -> str: ...

    def __format__(self, fmt: str, /) -> str: ...
    def isoformat(self) -> str: ...
    def timetuple(self) -> struct_time: ...
    def toordinal(self) -> int: ...
    if sys.version_info >= (3, 13):
        def __replace__(self, /, *, year: SupportsIndex = ..., month: SupportsIndex = ..., day: SupportsIndex = ...) -> Self: ...

    def replace(self, year: SupportsIndex = ..., month: SupportsIndex = ..., day: SupportsIndex = ...) -> Self: ...
    def __le__(self, value: date, /) -> bool: ...
    def __lt__(self, value: date, /) -> bool: ...
    def __ge__(self, value: date, /) -> bool: ...
    def __gt__(self, value: date, /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __add__(self, value: timedelta, /) -> Self: ...
    def __radd__(self, value: timedelta, /) -> Self: ...

    @overload
    def __sub__(self, value: datetime, /) -> Never: ...
    @overload
    def __sub__(self, value: Self, /) -> timedelta: ...
    @overload
    def __sub__(self, value: timedelta, /) -> Self: ...

    def __hash__(self) -> int: ...
    def weekday(self) -> int: ...
    def isoweekday(self) -> int: ...
    def isocalendar(self) -> _IsoCalendarDate: ...

@disjoint_base
class time:
    min: ClassVar[time]
    max: ClassVar[time]
    resolution: ClassVar[timedelta]
    def __new__(
        cls,
        hour: SupportsIndex = 0,
        minute: SupportsIndex = 0,
        second: SupportsIndex = 0,
        microsecond: SupportsIndex = 0,
        tzinfo: _TzInfo | None = None,
        *,
        fold: int = 0,
    ) -> Self: ...
    @property
    def hour(self) -> int: ...
    @property
    def minute(self) -> int: ...
    @property
    def second(self) -> int: ...
    @property
    def microsecond(self) -> int: ...
    @property
    def tzinfo(self) -> _TzInfo | None: ...
    @property
    def fold(self) -> int: ...
    def __le__(self, value: time, /) -> bool: ...
    def __lt__(self, value: time, /) -> bool: ...
    def __ge__(self, value: time, /) -> bool: ...
    def __gt__(self, value: time, /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    def isoformat(
        self, timespec: Literal["auto", "hours", "minutes", "seconds", "milliseconds", "microseconds"] = "auto"
    ) -> str: ...

    if sys.version_info >= (3, 15):
        @classmethod
        def fromisoformat(cls, string: str, /) -> Self: ...
    else:
        @classmethod
        def fromisoformat(cls, time_string: str, /) -> Self: ...

    if sys.version_info >= (3, 14):
        if sys.version_info >= (3, 15):
            @classmethod
            def strptime(cls, string: str, format: str, /) -> Self: ...
        else:
            @classmethod
            def strptime(cls, date_string: str, format: str, /) -> Self: ...

    # On <3.12, the name of the parameter in the pure-Python implementation
    # didn't match the name in the C implementation,
    # meaning it is only *safe* to pass it as a keyword argument on 3.12+
    if sys.version_info >= (3, 12):
        def strftime(self, format: str) -> str: ...
    else:
        def strftime(self, format: str, /) -> str: ...

    def __format__(self, fmt: str, /) -> str: ...
    def utcoffset(self) -> timedelta | None: ...
    def tzname(self) -> str | None: ...
    def dst(self) -> timedelta | None: ...
    if sys.version_info >= (3, 13):
        def __replace__(
            self,
            /,
            *,
            hour: SupportsIndex = ...,
            minute: SupportsIndex = ...,
            second: SupportsIndex = ...,
            microsecond: SupportsIndex = ...,
            tzinfo: _TzInfo | None = ...,
            fold: int = ...,
        ) -> Self: ...

    def replace(
        self,
        hour: SupportsIndex = ...,
        minute: SupportsIndex = ...,
        second: SupportsIndex = ...,
        microsecond: SupportsIndex = ...,
        tzinfo: _TzInfo | None = ...,
        *,
        fold: int = ...,
    ) -> Self: ...

_Date: TypeAlias = date
_Time: TypeAlias = time

@disjoint_base
class timedelta:
    min: ClassVar[timedelta]
    max: ClassVar[timedelta]
    resolution: ClassVar[timedelta]
    def __new__(
        cls,
        days: float = 0,
        seconds: float = 0,
        microseconds: float = 0,
        milliseconds: float = 0,
        minutes: float = 0,
        hours: float = 0,
        weeks: float = 0,
    ) -> Self: ...
    @property
    def days(self) -> int: ...
    @property
    def seconds(self) -> int: ...
    @property
    def microseconds(self) -> int: ...
    def total_seconds(self) -> float: ...
    def __add__(self, value: timedelta, /) -> timedelta: ...
    def __radd__(self, value: timedelta, /) -> timedelta: ...
    def __sub__(self, value: timedelta, /) -> timedelta: ...
    def __rsub__(self, value: timedelta, /) -> timedelta: ...
    def __neg__(self) -> timedelta: ...
    def __pos__(self) -> timedelta: ...
    def __abs__(self) -> timedelta: ...
    def __mul__(self, value: float, /) -> timedelta: ...
    def __rmul__(self, value: float, /) -> timedelta: ...

    @overload
    def __floordiv__(self, value: timedelta, /) -> int: ...
    @overload
    def __floordiv__(self, value: int, /) -> timedelta: ...

    @overload
    def __truediv__(self, value: timedelta, /) -> float: ...
    @overload
    def __truediv__(self, value: float, /) -> timedelta: ...

    def __mod__(self, value: timedelta, /) -> timedelta: ...
    def __divmod__(self, value: timedelta, /) -> tuple[int, timedelta]: ...
    def __le__(self, value: timedelta, /) -> bool: ...
    def __lt__(self, value: timedelta, /) -> bool: ...
    def __ge__(self, value: timedelta, /) -> bool: ...
    def __gt__(self, value: timedelta, /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __bool__(self) -> bool: ...
    def __hash__(self) -> int: ...

_TzInfoT = TypeVar("_TzInfoT", bound=tzinfo | None, default=Any)

@disjoint_base
class datetime(date, Generic[_TzInfoT]):
    min: ClassVar[datetime[None]]
    max: ClassVar[datetime[None]]

    @overload
    def __new__(
        cls,
        year: SupportsIndex,
        month: SupportsIndex,
        day: SupportsIndex,
        hour: SupportsIndex = 0,
        minute: SupportsIndex = 0,
        second: SupportsIndex = 0,
        microsecond: SupportsIndex = 0,
        tzinfo: None = None,
        *,
        fold: int = 0,
    ) -> datetime[None]: ...
    @overload
    def __new__(
        cls,
        year: SupportsIndex,
        month: SupportsIndex,
        day: SupportsIndex,
        hour: SupportsIndex = 0,
        minute: SupportsIndex = 0,
        second: SupportsIndex = 0,
        microsecond: SupportsIndex = 0,
        *,
        tzinfo: _TzInfo,
        fold: int = 0,
    ) -> datetime[_TzInfo]: ...
    @overload
    def __new__(
        cls,
        year: SupportsIndex,
        month: SupportsIndex,
        day: SupportsIndex,
        hour: SupportsIndex,
        minute: SupportsIndex,
        second: SupportsIndex,
        microsecond: SupportsIndex,
        tzinfo: _TzInfo,
        *,
        fold: int = 0,
    ) -> datetime[_TzInfo]: ...

    @property
    def hour(self) -> int: ...
    @property
    def minute(self) -> int: ...
    @property
    def second(self) -> int: ...
    @property
    def microsecond(self) -> int: ...
    @property
    def tzinfo(self) -> _TzInfoT: ...
    @property
    def fold(self) -> int: ...
    # On <3.12, the name of the first parameter in the pure-Python implementation
    # didn't match the name in the C implementation,
    # meaning it is only *safe* to pass it as a keyword argument on 3.12+
    if sys.version_info >= (3, 12):
        @overload  # type: ignore[override]
        @classmethod
        def fromtimestamp(cls, timestamp: float, tz: None = None) -> datetime[None]: ...
        @overload
        @classmethod
        def fromtimestamp(cls, timestamp: float, tz: _TzInfo) -> datetime[_TzInfo]: ...
        @overload
        @classmethod
        def fromtimestamp(cls, timestamp: float, tz: _TzInfo | None) -> datetime[_TzInfo] | datetime[None]: ...
    else:
        @overload  # type: ignore[override]
        @classmethod
        def fromtimestamp(cls, timestamp: float, /, tz: None = None) -> datetime[None]: ...
        @overload
        @classmethod
        def fromtimestamp(cls, timestamp: float, /, tz: _TzInfo) -> datetime[_TzInfo]: ...
        @overload
        @classmethod
        def fromtimestamp(cls, timestamp: float, /, tz: _TzInfo | None) -> datetime[_TzInfo] | datetime[None]: ...

    # fromisoformat returns Self, but we can't specialize "Self"
    if sys.version_info >= (3, 15):
        @classmethod
        def fromisoformat(cls, string: str, /) -> datetime[_TzInfo] | datetime[None]: ...  # type: ignore[override]
    else:
        @classmethod
        def fromisoformat(cls, date_string: str, /) -> datetime[_TzInfo] | datetime[None]: ...  # type: ignore[override]

    @classmethod
    @deprecated("Use timezone-aware objects to represent datetimes in UTC; e.g. by calling .fromtimestamp(datetime.UTC)")
    def utcfromtimestamp(cls, t: float, /) -> datetime[None]: ...

    @overload
    @classmethod
    def now(cls, tz: None = None) -> datetime[None]: ...
    @overload
    @classmethod
    def now(cls, tz: _TzInfo) -> datetime[_TzInfo]: ...

    @classmethod
    def today(cls) -> datetime[None]: ...  # type: ignore[override]
    @classmethod
    @deprecated("Use timezone-aware objects to represent datetimes in UTC; e.g. by calling .now(datetime.UTC)")
    def utcnow(cls) -> datetime[None]: ...

    @overload
    @classmethod
    def combine(cls, date: _Date, time: _Time) -> datetime[_TzInfo] | datetime[None]: ...
    @overload
    @classmethod
    def combine(cls, date: _Date, time: _Time, tzinfo: None) -> datetime[None]: ...
    @overload
    @classmethod
    def combine(cls, date: _Date, time: _Time, tzinfo: _TzInfo) -> datetime[_TzInfo]: ...

    def timestamp(self) -> float: ...
    def utctimetuple(self) -> struct_time: ...
    def date(self) -> _Date: ...
    def time(self) -> _Time: ...
    def timetz(self) -> _Time: ...
    if sys.version_info >= (3, 13):
        def __replace__(
            self,
            /,
            *,
            year: SupportsIndex = ...,
            month: SupportsIndex = ...,
            day: SupportsIndex = ...,
            hour: SupportsIndex = ...,
            minute: SupportsIndex = ...,
            second: SupportsIndex = ...,
            microsecond: SupportsIndex = ...,
            tzinfo: _TzInfo | None = ...,
            fold: int = ...,
        ) -> Self: ...

    @overload
    def replace(
        self,
        year: SupportsIndex = ...,
        month: SupportsIndex = ...,
        day: SupportsIndex = ...,
        hour: SupportsIndex = ...,
        minute: SupportsIndex = ...,
        second: SupportsIndex = ...,
        microsecond: SupportsIndex = ...,
        *,
        fold: int = ...,
    ) -> Self: ...
    @overload
    def replace(
        self,
        year: SupportsIndex,
        month: SupportsIndex,
        day: SupportsIndex,
        hour: SupportsIndex,
        minute: SupportsIndex,
        second: SupportsIndex,
        microsecond: SupportsIndex,
        tzinfo: _TzInfo,
        *,
        fold: int = ...,
    ) -> datetime[_TzInfo]: ...
    @overload
    def replace(
        self,
        year: SupportsIndex = ...,
        month: SupportsIndex = ...,
        day: SupportsIndex = ...,
        hour: SupportsIndex = ...,
        minute: SupportsIndex = ...,
        second: SupportsIndex = ...,
        microsecond: SupportsIndex = ...,
        *,
        tzinfo: _TzInfo,
        fold: int = ...,
    ) -> datetime[_TzInfo]: ...
    @overload
    def replace(
        self,
        year: SupportsIndex,
        month: SupportsIndex,
        day: SupportsIndex,
        hour: SupportsIndex,
        minute: SupportsIndex,
        second: SupportsIndex,
        microsecond: SupportsIndex,
        tzinfo: None,
        *,
        fold: int = ...,
    ) -> datetime[None]: ...
    @overload
    def replace(
        self,
        year: SupportsIndex = ...,
        month: SupportsIndex = ...,
        day: SupportsIndex = ...,
        hour: SupportsIndex = ...,
        minute: SupportsIndex = ...,
        second: SupportsIndex = ...,
        microsecond: SupportsIndex = ...,
        *,
        tzinfo: None,
        fold: int = ...,
    ) -> datetime[None]: ...

    def astimezone(self, tz: _TzInfo | None = None) -> datetime[_TzInfo]: ...
    def isoformat(
        self, sep: str = "T", timespec: Literal["auto", "hours", "minutes", "seconds", "milliseconds", "microseconds"] = "auto"
    ) -> str: ...

    if sys.version_info >= (3, 15):
        @classmethod
        def strptime(cls, string: str, format: str, /) -> Self: ...
    else:
        @classmethod
        def strptime(cls, date_string: str, format: str, /) -> Self: ...

    def utcoffset(self) -> timedelta | None: ...
    def tzname(self) -> str | None: ...
    def dst(self) -> timedelta | None: ...

    @overload  # type: ignore[override]
    def __le__(  # type: ignore[overload-overlap]
        self: datetime[_TzInfo] | datetime[_TzInfo | None], value: datetime[_TzInfo] | datetime[_TzInfo | None], /
    ) -> bool: ...  # type: ignore[misc]
    @overload
    def __le__(self: datetime[None] | datetime[_TzInfo | None], value: datetime[None] | datetime[_TzInfo | None], /) -> bool: ...  # type: ignore[misc]
    @overload
    def __le__(self: datetime[Any], value: datetime[Any], /) -> Never: ...

    @overload  # type: ignore[override]
    def __lt__(  # type: ignore[overload-overlap]
        self: datetime[_TzInfo] | datetime[_TzInfo | None], value: datetime[_TzInfo] | datetime[_TzInfo | None], /
    ) -> bool: ...  # type: ignore[misc]
    @overload
    def __lt__(self: datetime[None] | datetime[_TzInfo | None], value: datetime[None] | datetime[_TzInfo | None], /) -> bool: ...  # type: ignore[misc]
    @overload
    def __lt__(self: datetime[Any], value: datetime[Any], /) -> Never: ...

    @overload  # type: ignore[override]
    def __ge__(  # type: ignore[overload-overlap]
        self: datetime[_TzInfo] | datetime[_TzInfo | None], value: datetime[_TzInfo] | datetime[_TzInfo | None], /
    ) -> bool: ...  # type: ignore[misc]
    @overload
    def __ge__(self: datetime[None] | datetime[_TzInfo | None], value: datetime[None] | datetime[_TzInfo | None], /) -> bool: ...  # type: ignore[misc]
    @overload
    def __ge__(self: datetime[Any], value: datetime[Any], /) -> Never: ...

    @overload  # type: ignore[override]
    def __gt__(  # type: ignore[overload-overlap]
        self: datetime[_TzInfo] | datetime[_TzInfo | None], value: datetime[_TzInfo] | datetime[_TzInfo | None], /
    ) -> bool: ...  # type: ignore[misc]
    @overload
    def __gt__(self: datetime[None] | datetime[_TzInfo | None], value: datetime[None] | datetime[_TzInfo | None], /) -> bool: ...  # type: ignore[misc]
    @overload
    def __gt__(self: datetime[Any], value: datetime[Any], /) -> Never: ...

    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...

    @overload  # type: ignore[override]
    def __sub__(  # type: ignore[overload-overlap]
        self: datetime[_TzInfo] | datetime[_TzInfo | None], value: datetime[_TzInfo] | datetime[_TzInfo | None], /
    ) -> timedelta: ...
    @overload
    def __sub__(  # type: ignore[overload-overlap]
        self: datetime[None] | datetime[_TzInfo | None], value: datetime[None] | datetime[_TzInfo | None], /
    ) -> timedelta: ...
    @overload
    def __sub__(self: datetime[Any], value: datetime[Any], /) -> Never: ...
    @overload
    def __sub__(self, value: timedelta, /) -> Self: ...

datetime_CAPI: CapsuleType

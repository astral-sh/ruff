"""Specific date/time and related types.

See https://data.iana.org/time-zones/tz-link.html for
time zone and DST data sources.
"""

import sys
from abc import abstractmethod
from time import struct_time
from typing import ClassVar, Final, NoReturn, SupportsIndex, final, overload, type_check_only
from typing_extensions import CapsuleType, Self, TypeAlias, deprecated, disjoint_base

if sys.version_info >= (3, 11):
    __all__ = ("date", "datetime", "time", "timedelta", "timezone", "tzinfo", "MINYEAR", "MAXYEAR", "UTC")
else:
    __all__ = ("date", "datetime", "time", "timedelta", "timezone", "tzinfo", "MINYEAR", "MAXYEAR")

MINYEAR: Final = 1
MAXYEAR: Final = 9999

class tzinfo:
    """Abstract base class for time zone info objects."""

    @abstractmethod
    def tzname(self, dt: datetime | None, /) -> str | None:
        """datetime -> string name of time zone."""

    @abstractmethod
    def utcoffset(self, dt: datetime | None, /) -> timedelta | None:
        """datetime -> timedelta showing offset from UTC, negative values indicating West of UTC"""

    @abstractmethod
    def dst(self, dt: datetime | None, /) -> timedelta | None:
        """datetime -> DST offset as timedelta positive east of UTC."""

    def fromutc(self, dt: datetime, /) -> datetime:
        """datetime in UTC -> datetime in local time."""

# Alias required to avoid name conflicts with date(time).tzinfo.
_TzInfo: TypeAlias = tzinfo

@final
class timezone(tzinfo):
    """Fixed offset from UTC implementation of tzinfo."""

    utc: ClassVar[timezone]
    min: ClassVar[timezone]
    max: ClassVar[timezone]
    def __new__(cls, offset: timedelta, name: str = ...) -> Self: ...
    def tzname(self, dt: datetime | None, /) -> str:
        """If name is specified when timezone is created, returns the name.  Otherwise returns offset as 'UTC(+|-)HH:MM'."""

    def utcoffset(self, dt: datetime | None, /) -> timedelta:
        """Return fixed offset."""

    def dst(self, dt: datetime | None, /) -> None:
        """Return None."""

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
    """date(year, month, day) --> date object"""

    min: ClassVar[date]
    max: ClassVar[date]
    resolution: ClassVar[timedelta]
    def __new__(cls, year: SupportsIndex, month: SupportsIndex, day: SupportsIndex) -> Self: ...
    @classmethod
    def fromtimestamp(cls, timestamp: float, /) -> Self:
        """Create a date from a POSIX timestamp.

        The timestamp is a number, e.g. created via time.time(), that is interpreted
        as local time.
        """

    @classmethod
    def today(cls) -> Self:
        """Current date or datetime:  same as self.__class__.fromtimestamp(time.time())."""

    @classmethod
    def fromordinal(cls, n: int, /) -> Self:
        """int -> date corresponding to a proleptic Gregorian ordinal."""

    @classmethod
    def fromisoformat(cls, date_string: str, /) -> Self:
        """str -> Construct a date from a string in ISO 8601 format."""

    @classmethod
    def fromisocalendar(cls, year: int, week: int, day: int) -> Self:
        """int, int, int -> Construct a date from the ISO year, week number and weekday.

        This is the inverse of the date.isocalendar() function
        """

    @property
    def year(self) -> int: ...
    @property
    def month(self) -> int: ...
    @property
    def day(self) -> int: ...
    def ctime(self) -> str:
        """Return ctime() style string."""
    if sys.version_info >= (3, 14):
        @classmethod
        def strptime(cls, date_string: str, format: str, /) -> Self:
            """string, format -> new date parsed from a string (like time.strptime())."""
    # On <3.12, the name of the parameter in the pure-Python implementation
    # didn't match the name in the C implementation,
    # meaning it is only *safe* to pass it as a keyword argument on 3.12+
    if sys.version_info >= (3, 12):
        def strftime(self, format: str) -> str:
            """format -> strftime() style string."""
    else:
        def strftime(self, format: str, /) -> str:
            """format -> strftime() style string."""

    def __format__(self, fmt: str, /) -> str:
        """Formats self with strftime."""

    def isoformat(self) -> str:
        """Return string in ISO 8601 format, YYYY-MM-DD."""

    def timetuple(self) -> struct_time:
        """Return time tuple, compatible with time.localtime()."""

    def toordinal(self) -> int:
        """Return proleptic Gregorian ordinal.  January 1 of year 1 is day 1."""
    if sys.version_info >= (3, 13):
        def __replace__(self, /, *, year: SupportsIndex = ..., month: SupportsIndex = ..., day: SupportsIndex = ...) -> Self:
            """The same as replace()."""

    def replace(self, year: SupportsIndex = ..., month: SupportsIndex = ..., day: SupportsIndex = ...) -> Self:
        """Return date with new specified fields."""

    def __le__(self, value: date, /) -> bool: ...
    def __lt__(self, value: date, /) -> bool: ...
    def __ge__(self, value: date, /) -> bool: ...
    def __gt__(self, value: date, /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __add__(self, value: timedelta, /) -> Self:
        """Return self+value."""

    def __radd__(self, value: timedelta, /) -> Self:
        """Return value+self."""

    @overload
    def __sub__(self, value: datetime, /) -> NoReturn:
        """Return self-value."""

    @overload
    def __sub__(self, value: Self, /) -> timedelta: ...
    @overload
    def __sub__(self, value: timedelta, /) -> Self: ...
    def __hash__(self) -> int: ...
    def weekday(self) -> int:
        """Return the day of the week represented by the date.
        Monday == 0 ... Sunday == 6
        """

    def isoweekday(self) -> int:
        """Return the day of the week represented by the date.
        Monday == 1 ... Sunday == 7
        """

    def isocalendar(self) -> _IsoCalendarDate:
        """Return a named tuple containing ISO year, week number, and weekday."""

@disjoint_base
class time:
    """time([hour[, minute[, second[, microsecond[, tzinfo]]]]]) --> a time object

    All arguments are optional. tzinfo may be None, or an instance of
    a tzinfo subclass. The remaining arguments may be ints.
    """

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
    def isoformat(self, timespec: str = "auto") -> str:
        """Return string in ISO 8601 format, [HH[:MM[:SS[.mmm[uuu]]]]][+HH:MM].

        The optional argument timespec specifies the number of additional terms
        of the time to include. Valid options are 'auto', 'hours', 'minutes',
        'seconds', 'milliseconds' and 'microseconds'.
        """

    @classmethod
    def fromisoformat(cls, time_string: str, /) -> Self:
        """string -> time from a string in ISO 8601 format"""
    if sys.version_info >= (3, 14):
        @classmethod
        def strptime(cls, date_string: str, format: str, /) -> Self:
            """string, format -> new time parsed from a string (like time.strptime())."""
    # On <3.12, the name of the parameter in the pure-Python implementation
    # didn't match the name in the C implementation,
    # meaning it is only *safe* to pass it as a keyword argument on 3.12+
    if sys.version_info >= (3, 12):
        def strftime(self, format: str) -> str:
            """format -> strftime() style string."""
    else:
        def strftime(self, format: str, /) -> str:
            """format -> strftime() style string."""

    def __format__(self, fmt: str, /) -> str:
        """Formats self with strftime."""

    def utcoffset(self) -> timedelta | None:
        """Return self.tzinfo.utcoffset(self)."""

    def tzname(self) -> str | None:
        """Return self.tzinfo.tzname(self)."""

    def dst(self) -> timedelta | None:
        """Return self.tzinfo.dst(self)."""
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
        ) -> Self:
            """The same as replace()."""

    def replace(
        self,
        hour: SupportsIndex = ...,
        minute: SupportsIndex = ...,
        second: SupportsIndex = ...,
        microsecond: SupportsIndex = ...,
        tzinfo: _TzInfo | None = ...,
        *,
        fold: int = ...,
    ) -> Self:
        """Return time with new specified fields."""

_Date: TypeAlias = date
_Time: TypeAlias = time

@disjoint_base
class timedelta:
    """Difference between two datetime values.

    timedelta(days=0, seconds=0, microseconds=0, milliseconds=0, minutes=0, hours=0, weeks=0)

    All arguments are optional and default to 0.
    Arguments may be integers or floats, and may be positive or negative.
    """

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
    def days(self) -> int:
        """Number of days."""

    @property
    def seconds(self) -> int:
        """Number of seconds (>= 0 and less than 1 day)."""

    @property
    def microseconds(self) -> int:
        """Number of microseconds (>= 0 and less than 1 second)."""

    def total_seconds(self) -> float:
        """Total seconds in the duration."""

    def __add__(self, value: timedelta, /) -> timedelta:
        """Return self+value."""

    def __radd__(self, value: timedelta, /) -> timedelta:
        """Return value+self."""

    def __sub__(self, value: timedelta, /) -> timedelta:
        """Return self-value."""

    def __rsub__(self, value: timedelta, /) -> timedelta:
        """Return value-self."""

    def __neg__(self) -> timedelta:
        """-self"""

    def __pos__(self) -> timedelta:
        """+self"""

    def __abs__(self) -> timedelta:
        """abs(self)"""

    def __mul__(self, value: float, /) -> timedelta:
        """Return self*value."""

    def __rmul__(self, value: float, /) -> timedelta:
        """Return value*self."""

    @overload
    def __floordiv__(self, value: timedelta, /) -> int:
        """Return self//value."""

    @overload
    def __floordiv__(self, value: int, /) -> timedelta: ...
    @overload
    def __truediv__(self, value: timedelta, /) -> float:
        """Return self/value."""

    @overload
    def __truediv__(self, value: float, /) -> timedelta: ...
    def __mod__(self, value: timedelta, /) -> timedelta:
        """Return self%value."""

    def __divmod__(self, value: timedelta, /) -> tuple[int, timedelta]:
        """Return divmod(self, value)."""

    def __le__(self, value: timedelta, /) -> bool: ...
    def __lt__(self, value: timedelta, /) -> bool: ...
    def __ge__(self, value: timedelta, /) -> bool: ...
    def __gt__(self, value: timedelta, /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __bool__(self) -> bool:
        """True if self else False"""

    def __hash__(self) -> int: ...

@disjoint_base
class datetime(date):
    """datetime(year, month, day[, hour[, minute[, second[, microsecond[,tzinfo]]]]])

    The year, month and day arguments are required. tzinfo may be None, or an
    instance of a tzinfo subclass. The remaining arguments may be ints.
    """

    min: ClassVar[datetime]
    max: ClassVar[datetime]
    def __new__(
        cls,
        year: SupportsIndex,
        month: SupportsIndex,
        day: SupportsIndex,
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
    # On <3.12, the name of the first parameter in the pure-Python implementation
    # didn't match the name in the C implementation,
    # meaning it is only *safe* to pass it as a keyword argument on 3.12+
    if sys.version_info >= (3, 12):
        @classmethod
        def fromtimestamp(cls, timestamp: float, tz: _TzInfo | None = None) -> Self:
            """timestamp[, tz] -> tz's local time from POSIX timestamp."""
    else:
        @classmethod
        def fromtimestamp(cls, timestamp: float, /, tz: _TzInfo | None = None) -> Self:
            """timestamp[, tz] -> tz's local time from POSIX timestamp."""

    @classmethod
    @deprecated("Use timezone-aware objects to represent datetimes in UTC; e.g. by calling .fromtimestamp(datetime.timezone.utc)")
    def utcfromtimestamp(cls, t: float, /) -> Self:
        """Construct a naive UTC datetime from a POSIX timestamp."""

    @classmethod
    def now(cls, tz: _TzInfo | None = None) -> Self:
        """Returns new datetime object representing current time local to tz.

          tz
            Timezone object.

        If no tz is specified, uses local timezone.
        """

    @classmethod
    @deprecated("Use timezone-aware objects to represent datetimes in UTC; e.g. by calling .now(datetime.timezone.utc)")
    def utcnow(cls) -> Self:
        """Return a new datetime representing UTC day and time."""

    @classmethod
    def combine(cls, date: _Date, time: _Time, tzinfo: _TzInfo | None = ...) -> Self:
        """date, time -> datetime with same date and time fields"""

    def timestamp(self) -> float:
        """Return POSIX timestamp as float."""

    def utctimetuple(self) -> struct_time:
        """Return UTC time tuple, compatible with time.localtime()."""

    def date(self) -> _Date:
        """Return date object with same year, month and day."""

    def time(self) -> _Time:
        """Return time object with same time but with tzinfo=None."""

    def timetz(self) -> _Time:
        """Return time object with same time and tzinfo."""
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
        ) -> Self:
            """The same as replace()."""

    def replace(
        self,
        year: SupportsIndex = ...,
        month: SupportsIndex = ...,
        day: SupportsIndex = ...,
        hour: SupportsIndex = ...,
        minute: SupportsIndex = ...,
        second: SupportsIndex = ...,
        microsecond: SupportsIndex = ...,
        tzinfo: _TzInfo | None = ...,
        *,
        fold: int = ...,
    ) -> Self:
        """Return datetime with new specified fields."""

    def astimezone(self, tz: _TzInfo | None = None) -> Self:
        """tz -> convert to local time in new timezone tz"""

    def isoformat(self, sep: str = "T", timespec: str = "auto") -> str:
        """[sep] -> string in ISO 8601 format, YYYY-MM-DDT[HH[:MM[:SS[.mmm[uuu]]]]][+HH:MM].
        sep is used to separate the year from the time, and defaults to 'T'.
        The optional argument timespec specifies the number of additional terms
        of the time to include. Valid options are 'auto', 'hours', 'minutes',
        'seconds', 'milliseconds' and 'microseconds'.
        """

    @classmethod
    def strptime(cls, date_string: str, format: str, /) -> Self:
        """string, format -> new datetime parsed from a string (like time.strptime())."""

    def utcoffset(self) -> timedelta | None:
        """Return self.tzinfo.utcoffset(self)."""

    def tzname(self) -> str | None:
        """Return self.tzinfo.tzname(self)."""

    def dst(self) -> timedelta | None:
        """Return self.tzinfo.dst(self)."""

    def __le__(self, value: datetime, /) -> bool: ...  # type: ignore[override]
    def __lt__(self, value: datetime, /) -> bool: ...  # type: ignore[override]
    def __ge__(self, value: datetime, /) -> bool: ...  # type: ignore[override]
    def __gt__(self, value: datetime, /) -> bool: ...  # type: ignore[override]
    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    @overload  # type: ignore[override]
    def __sub__(self, value: Self, /) -> timedelta:
        """Return self-value."""

    @overload
    def __sub__(self, value: timedelta, /) -> Self: ...

datetime_CAPI: CapsuleType

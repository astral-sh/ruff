"""Calendar printing functions

Note when comparing these calendars to the ones printed by cal(1): By
default, these calendars have Monday as the first day of the week, and
Sunday as the last (the European convention). Use setfirstweekday() to
set the first day of the week (0=Monday, 6=Sunday).
"""

import datetime
import enum
import sys
from _typeshed import Unused
from collections.abc import Iterable, Sequence
from time import struct_time
from typing import ClassVar, Final
from typing_extensions import TypeAlias

__all__ = [
    "IllegalMonthError",
    "IllegalWeekdayError",
    "setfirstweekday",
    "firstweekday",
    "isleap",
    "leapdays",
    "weekday",
    "monthrange",
    "monthcalendar",
    "prmonth",
    "month",
    "prcal",
    "calendar",
    "timegm",
    "month_name",
    "month_abbr",
    "day_name",
    "day_abbr",
    "Calendar",
    "TextCalendar",
    "HTMLCalendar",
    "LocaleTextCalendar",
    "LocaleHTMLCalendar",
    "weekheader",
]

if sys.version_info >= (3, 10):
    __all__ += ["FRIDAY", "MONDAY", "SATURDAY", "SUNDAY", "THURSDAY", "TUESDAY", "WEDNESDAY"]
if sys.version_info >= (3, 12):
    __all__ += [
        "Day",
        "Month",
        "JANUARY",
        "FEBRUARY",
        "MARCH",
        "APRIL",
        "MAY",
        "JUNE",
        "JULY",
        "AUGUST",
        "SEPTEMBER",
        "OCTOBER",
        "NOVEMBER",
        "DECEMBER",
    ]

_LocaleType: TypeAlias = tuple[str | None, str | None]

class IllegalMonthError(ValueError, IndexError):
    month: int
    def __init__(self, month: int) -> None: ...

class IllegalWeekdayError(ValueError):
    weekday: int
    def __init__(self, weekday: int) -> None: ...

def isleap(year: int) -> bool:
    """Return True for leap years, False for non-leap years."""

def leapdays(y1: int, y2: int) -> int:
    """Return number of leap years in range [y1, y2).
    Assume y1 <= y2.
    """

def weekday(year: int, month: int, day: int) -> int:
    """Return weekday (0-6 ~ Mon-Sun) for year, month (1-12), day (1-31)."""

def monthrange(year: int, month: int) -> tuple[int, int]:
    """Return weekday of first day of month (0-6 ~ Mon-Sun)
    and number of days (28-31) for year, month.
    """

class Calendar:
    """
    Base calendar class. This class doesn't do any formatting. It simply
    provides data to subclasses.
    """

    firstweekday: int
    def __init__(self, firstweekday: int = 0) -> None: ...
    def getfirstweekday(self) -> int: ...
    def setfirstweekday(self, firstweekday: int) -> None: ...
    def iterweekdays(self) -> Iterable[int]:
        """
        Return an iterator for one week of weekday numbers starting with the
        configured first one.
        """

    def itermonthdates(self, year: int, month: int) -> Iterable[datetime.date]:
        """
        Return an iterator for one month. The iterator will yield datetime.date
        values and will always iterate through complete weeks, so it will yield
        dates outside the specified month.
        """

    def itermonthdays2(self, year: int, month: int) -> Iterable[tuple[int, int]]:
        """
        Like itermonthdates(), but will yield (day number, weekday number)
        tuples. For days outside the specified month the day number is 0.
        """

    def itermonthdays(self, year: int, month: int) -> Iterable[int]:
        """
        Like itermonthdates(), but will yield day numbers. For days outside
        the specified month the day number is 0.
        """

    def monthdatescalendar(self, year: int, month: int) -> list[list[datetime.date]]:
        """
        Return a matrix (list of lists) representing a month's calendar.
        Each row represents a week; week entries are datetime.date values.
        """

    def monthdays2calendar(self, year: int, month: int) -> list[list[tuple[int, int]]]:
        """
        Return a matrix representing a month's calendar.
        Each row represents a week; week entries are
        (day number, weekday number) tuples. Day numbers outside this month
        are zero.
        """

    def monthdayscalendar(self, year: int, month: int) -> list[list[int]]:
        """
        Return a matrix representing a month's calendar.
        Each row represents a week; days outside this month are zero.
        """

    def yeardatescalendar(self, year: int, width: int = 3) -> list[list[list[list[datetime.date]]]]:
        """
        Return the data for the specified year ready for formatting. The return
        value is a list of month rows. Each month row contains up to width months.
        Each month contains between 4 and 6 weeks and each week contains 1-7
        days. Days are datetime.date objects.
        """

    def yeardays2calendar(self, year: int, width: int = 3) -> list[list[list[list[tuple[int, int]]]]]:
        """
        Return the data for the specified year ready for formatting (similar to
        yeardatescalendar()). Entries in the week lists are
        (day number, weekday number) tuples. Day numbers outside this month are
        zero.
        """

    def yeardayscalendar(self, year: int, width: int = 3) -> list[list[list[list[int]]]]:
        """
        Return the data for the specified year ready for formatting (similar to
        yeardatescalendar()). Entries in the week lists are day numbers.
        Day numbers outside this month are zero.
        """

    def itermonthdays3(self, year: int, month: int) -> Iterable[tuple[int, int, int]]:
        """
        Like itermonthdates(), but will yield (year, month, day) tuples.  Can be
        used for dates outside of datetime.date range.
        """

    def itermonthdays4(self, year: int, month: int) -> Iterable[tuple[int, int, int, int]]:
        """
        Like itermonthdates(), but will yield (year, month, day, day_of_week) tuples.
        Can be used for dates outside of datetime.date range.
        """

class TextCalendar(Calendar):
    """
    Subclass of Calendar that outputs a calendar as a simple plain text
    similar to the UNIX program cal.
    """

    def prweek(self, theweek: int, width: int) -> None:
        """
        Print a single week (no newline).
        """

    def formatday(self, day: int, weekday: int, width: int) -> str:
        """
        Returns a formatted day.
        """

    def formatweek(self, theweek: int, width: int) -> str:
        """
        Returns a single week in a string (no newline).
        """

    def formatweekday(self, day: int, width: int) -> str:
        """
        Returns a formatted week day name.
        """

    def formatweekheader(self, width: int) -> str:
        """
        Return a header for a week.
        """

    def formatmonthname(self, theyear: int, themonth: int, width: int, withyear: bool = True) -> str:
        """
        Return a formatted month name.
        """

    def prmonth(self, theyear: int, themonth: int, w: int = 0, l: int = 0) -> None:
        """
        Print a month's calendar.
        """

    def formatmonth(self, theyear: int, themonth: int, w: int = 0, l: int = 0) -> str:
        """
        Return a month's calendar string (multi-line).
        """

    def formatyear(self, theyear: int, w: int = 2, l: int = 1, c: int = 6, m: int = 3) -> str:
        """
        Returns a year's calendar as a multi-line string.
        """

    def pryear(self, theyear: int, w: int = 0, l: int = 0, c: int = 6, m: int = 3) -> None:
        """Print a year's calendar."""

def firstweekday() -> int: ...
def monthcalendar(year: int, month: int) -> list[list[int]]:
    """
    Return a matrix representing a month's calendar.
    Each row represents a week; days outside this month are zero.
    """

def prweek(theweek: int, width: int) -> None:
    """
    Print a single week (no newline).
    """

def week(theweek: int, width: int) -> str:
    """
    Returns a single week in a string (no newline).
    """

def weekheader(width: int) -> str:
    """
    Return a header for a week.
    """

def prmonth(theyear: int, themonth: int, w: int = 0, l: int = 0) -> None:
    """
    Print a month's calendar.
    """

def month(theyear: int, themonth: int, w: int = 0, l: int = 0) -> str:
    """
    Return a month's calendar string (multi-line).
    """

def calendar(theyear: int, w: int = 2, l: int = 1, c: int = 6, m: int = 3) -> str:
    """
    Returns a year's calendar as a multi-line string.
    """

def prcal(theyear: int, w: int = 0, l: int = 0, c: int = 6, m: int = 3) -> None:
    """Print a year's calendar."""

class HTMLCalendar(Calendar):
    """
    This calendar returns complete HTML pages.
    """

    cssclasses: ClassVar[list[str]]
    cssclass_noday: ClassVar[str]
    cssclasses_weekday_head: ClassVar[list[str]]
    cssclass_month_head: ClassVar[str]
    cssclass_month: ClassVar[str]
    cssclass_year: ClassVar[str]
    cssclass_year_head: ClassVar[str]
    def formatday(self, day: int, weekday: int) -> str:
        """
        Return a day as a table cell.
        """

    def formatweek(self, theweek: int) -> str:
        """
        Return a complete week as a table row.
        """

    def formatweekday(self, day: int) -> str:
        """
        Return a weekday name as a table header.
        """

    def formatweekheader(self) -> str:
        """
        Return a header for a week as a table row.
        """

    def formatmonthname(self, theyear: int, themonth: int, withyear: bool = True) -> str:
        """
        Return a month name as a table row.
        """

    def formatmonth(self, theyear: int, themonth: int, withyear: bool = True) -> str:
        """
        Return a formatted month as a table.
        """

    def formatyear(self, theyear: int, width: int = 3) -> str:
        """
        Return a formatted year as a table of tables.
        """

    def formatyearpage(
        self, theyear: int, width: int = 3, css: str | None = "calendar.css", encoding: str | None = None
    ) -> bytes:
        """
        Return a formatted year as a complete HTML page.
        """

class different_locale:
    def __init__(self, locale: _LocaleType) -> None: ...
    def __enter__(self) -> None: ...
    def __exit__(self, *args: Unused) -> None: ...

class LocaleTextCalendar(TextCalendar):
    """
    This class can be passed a locale name in the constructor and will return
    month and weekday names in the specified locale.
    """

    def __init__(self, firstweekday: int = 0, locale: _LocaleType | None = None) -> None: ...

class LocaleHTMLCalendar(HTMLCalendar):
    """
    This class can be passed a locale name in the constructor and will return
    month and weekday names in the specified locale.
    """

    def __init__(self, firstweekday: int = 0, locale: _LocaleType | None = None) -> None: ...
    def formatweekday(self, day: int) -> str: ...
    def formatmonthname(self, theyear: int, themonth: int, withyear: bool = True) -> str: ...

c: TextCalendar

def setfirstweekday(firstweekday: int) -> None: ...
def format(cols: int, colwidth: int = 20, spacing: int = 6) -> str:
    """Prints multi-column formatting for year calendars"""

def formatstring(cols: int, colwidth: int = 20, spacing: int = 6) -> str:
    """Returns a string formatted from n strings, centered within n columns."""

def timegm(tuple: tuple[int, ...] | struct_time) -> int:
    """Unrelated but handy function to calculate Unix timestamp from GMT."""

# Data attributes
day_name: Sequence[str]
day_abbr: Sequence[str]
month_name: Sequence[str]
month_abbr: Sequence[str]

if sys.version_info >= (3, 12):
    class Month(enum.IntEnum):
        JANUARY = 1
        FEBRUARY = 2
        MARCH = 3
        APRIL = 4
        MAY = 5
        JUNE = 6
        JULY = 7
        AUGUST = 8
        SEPTEMBER = 9
        OCTOBER = 10
        NOVEMBER = 11
        DECEMBER = 12

    JANUARY: Final = Month.JANUARY
    FEBRUARY: Final = Month.FEBRUARY
    MARCH: Final = Month.MARCH
    APRIL: Final = Month.APRIL
    MAY: Final = Month.MAY
    JUNE: Final = Month.JUNE
    JULY: Final = Month.JULY
    AUGUST: Final = Month.AUGUST
    SEPTEMBER: Final = Month.SEPTEMBER
    OCTOBER: Final = Month.OCTOBER
    NOVEMBER: Final = Month.NOVEMBER
    DECEMBER: Final = Month.DECEMBER

    class Day(enum.IntEnum):
        MONDAY = 0
        TUESDAY = 1
        WEDNESDAY = 2
        THURSDAY = 3
        FRIDAY = 4
        SATURDAY = 5
        SUNDAY = 6

    MONDAY: Final = Day.MONDAY
    TUESDAY: Final = Day.TUESDAY
    WEDNESDAY: Final = Day.WEDNESDAY
    THURSDAY: Final = Day.THURSDAY
    FRIDAY: Final = Day.FRIDAY
    SATURDAY: Final = Day.SATURDAY
    SUNDAY: Final = Day.SUNDAY
else:
    MONDAY: Final = 0
    TUESDAY: Final = 1
    WEDNESDAY: Final = 2
    THURSDAY: Final = 3
    FRIDAY: Final = 4
    SATURDAY: Final = 5
    SUNDAY: Final = 6

EPOCH: Final = 1970

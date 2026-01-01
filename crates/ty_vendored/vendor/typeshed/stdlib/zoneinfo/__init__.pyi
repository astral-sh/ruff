import sys
from collections.abc import Iterable
from datetime import datetime, timedelta, tzinfo
from typing_extensions import Self, disjoint_base
from zoneinfo._common import ZoneInfoNotFoundError as ZoneInfoNotFoundError, _IOBytes
from zoneinfo._tzpath import (
    TZPATH as TZPATH,
    InvalidTZPathWarning as InvalidTZPathWarning,
    available_timezones as available_timezones,
    reset_tzpath as reset_tzpath,
)

__all__ = ["ZoneInfo", "reset_tzpath", "available_timezones", "TZPATH", "ZoneInfoNotFoundError", "InvalidTZPathWarning"]

@disjoint_base
class ZoneInfo(tzinfo):
    @property
    def key(self) -> str: ...
    def __new__(cls, key: str) -> Self: ...
    @classmethod
    def no_cache(cls, key: str) -> Self:
        """Get a new instance of ZoneInfo, bypassing the cache."""
    if sys.version_info >= (3, 12):
        @classmethod
        def from_file(cls, file_obj: _IOBytes, /, key: str | None = None) -> Self:
            """Create a ZoneInfo file from a file object."""
    else:
        @classmethod
        def from_file(cls, fobj: _IOBytes, /, key: str | None = None) -> Self:
            """Create a ZoneInfo file from a file object."""

    @classmethod
    def clear_cache(cls, *, only_keys: Iterable[str] | None = None) -> None:
        """Clear the ZoneInfo cache."""

    def tzname(self, dt: datetime | None, /) -> str | None:
        """Retrieve a string containing the abbreviation for the time zone that applies in a zone at a given datetime."""

    def utcoffset(self, dt: datetime | None, /) -> timedelta | None:
        """Retrieve a timedelta representing the UTC offset in a zone at the given datetime."""

    def dst(self, dt: datetime | None, /) -> timedelta | None:
        """Retrieve a timedelta representing the amount of DST applied in a zone at the given datetime."""

def __dir__() -> list[str]: ...

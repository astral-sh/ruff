import sys
from collections.abc import Iterable
from datetime import datetime, timedelta, tzinfo
from typing_extensions import Self
from zoneinfo._common import ZoneInfoNotFoundError as ZoneInfoNotFoundError, _IOBytes
from zoneinfo._tzpath import (
    TZPATH as TZPATH,
    InvalidTZPathWarning as InvalidTZPathWarning,
    available_timezones as available_timezones,
    reset_tzpath as reset_tzpath,
)

__all__ = ["ZoneInfo", "reset_tzpath", "available_timezones", "TZPATH", "ZoneInfoNotFoundError", "InvalidTZPathWarning"]

class ZoneInfo(tzinfo):
    @property
    def key(self) -> str: ...
    def __new__(cls, key: str) -> Self: ...
    @classmethod
    def no_cache(cls, key: str) -> Self: ...
    if sys.version_info >= (3, 12):
        @classmethod
        def from_file(cls, file_obj: _IOBytes, /, key: str | None = None) -> Self: ...
    else:
        @classmethod
        def from_file(cls, fobj: _IOBytes, /, key: str | None = None) -> Self: ...

    @classmethod
    def clear_cache(cls, *, only_keys: Iterable[str] | None = None) -> None: ...
    def tzname(self, dt: datetime | None, /) -> str | None: ...
    def utcoffset(self, dt: datetime | None, /) -> timedelta | None: ...
    def dst(self, dt: datetime | None, /) -> timedelta | None: ...

def __dir__() -> list[str]: ...

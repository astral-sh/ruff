from _typeshed import StrPath
from collections.abc import Sequence

# Note: Both here and in clear_cache, the types allow the use of `str` where
# a sequence of strings is required. This should be remedied if a solution
# to this typing bug is found: https://github.com/python/typing/issues/256
def reset_tzpath(to: Sequence[StrPath] | None = None) -> None:
    """Reset global TZPATH."""

def find_tzfile(key: str) -> str | None:
    """Retrieve the path to a TZif file from a key."""

def available_timezones() -> set[str]:
    """Returns a set containing all available time zones.

    .. caution::

        This may attempt to open a large number of files, since the best way to
        determine if a given file on the time zone search path is to open it
        and check for the "magic string" at the beginning.
    """

TZPATH: tuple[str, ...]

class InvalidTZPathWarning(RuntimeWarning):
    """Warning raised if an invalid path is specified in PYTHONTZPATH."""

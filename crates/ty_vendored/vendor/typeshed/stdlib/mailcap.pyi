"""Mailcap file handling.  See RFC 1524."""

from collections.abc import Mapping, Sequence
from typing_extensions import TypeAlias

_Cap: TypeAlias = dict[str, str | int]

__all__ = ["getcaps", "findmatch"]

def findmatch(
    caps: Mapping[str, list[_Cap]], MIMEtype: str, key: str = "view", filename: str = "/dev/null", plist: Sequence[str] = []
) -> tuple[str | None, _Cap | None]:
    """Find a match for a mailcap entry.

    Return a tuple containing the command line, and the mailcap entry
    used; (None, None) if no match is found.  This may invoke the
    'test' command of several matching entries before deciding which
    entry to use.

    """

def getcaps() -> dict[str, list[_Cap]]:
    """Return a dictionary containing the mailcap database.

    The dictionary maps a MIME type (in all lowercase, e.g. 'text/plain')
    to a list of dictionaries corresponding to mailcap entries.  The list
    collects all the entries for that MIME type from all available mailcap
    files.  Each dictionary contains key-value pairs for that MIME type,
    where the viewing command is stored with the key "view".

    """

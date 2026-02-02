import sys
from _typeshed import SupportsRead
from collections.abc import Callable
from typing import Any, overload
from typing_extensions import deprecated

__all__ = ("loads", "load", "TOMLDecodeError")

if sys.version_info >= (3, 14):
    class TOMLDecodeError(ValueError):
        """An error raised if a document is not valid TOML.

        Adds the following attributes to ValueError:
        msg: The unformatted error message
        doc: The TOML document being parsed
        pos: The index of doc where parsing failed
        lineno: The line corresponding to pos
        colno: The column corresponding to pos
        """

        msg: str
        doc: str
        pos: int
        lineno: int
        colno: int
        @overload
        def __init__(self, msg: str, doc: str, pos: int) -> None: ...
        @overload
        @deprecated("Deprecated since Python 3.14. Set the 'msg', 'doc' and 'pos' arguments only.")
        def __init__(self, msg: str | type = ..., doc: str | type = ..., pos: int | type = ..., *args: Any) -> None: ...

else:
    class TOMLDecodeError(ValueError):
        """An error raised if a document is not valid TOML."""

def load(fp: SupportsRead[bytes], /, *, parse_float: Callable[[str], Any] = ...) -> dict[str, Any]:
    """Parse TOML from a binary file object."""

def loads(s: str, /, *, parse_float: Callable[[str], Any] = ...) -> dict[str, Any]:
    """Parse TOML from a string."""

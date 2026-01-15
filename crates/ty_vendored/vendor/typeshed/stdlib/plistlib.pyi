"""plistlib.py -- a tool to generate and parse MacOSX .plist files.

The property list (.plist) file format is a simple XML pickle supporting
basic object types, like dictionaries, lists, numbers and strings.
Usually the top level object is a dictionary.

To write out a plist file, use the dump(value, file)
function. 'value' is the top level object, 'file' is
a (writable) file object.

To parse a plist from a file, use the load(file) function,
with a (readable) file object as the only argument. It
returns the top level object (again, usually a dictionary).

To work with plist data in bytes objects, you can use loads()
and dumps().

Values can be strings, integers, floats, booleans, tuples, lists,
dictionaries (but only with string keys), Data, bytes, bytearray, or
datetime.datetime objects.

Generate Plist example:

    import datetime
    import plistlib

    pl = dict(
        aString = "Doodah",
        aList = ["A", "B", 12, 32.1, [1, 2, 3]],
        aFloat = 0.1,
        anInt = 728,
        aDict = dict(
            anotherString = "<hello & hi there!>",
            aThirdString = "M\\xe4ssig, Ma\\xdf",
            aTrueValue = True,
            aFalseValue = False,
        ),
        someData = b"<binary gunk>",
        someMoreData = b"<lots of binary gunk>" * 10,
        aDate = datetime.datetime.now()
    )
    print(plistlib.dumps(pl).decode())

Parse Plist example:

    import plistlib

    plist = b'''<plist version="1.0">
    <dict>
        <key>foo</key>
        <string>bar</string>
    </dict>
    </plist>'''
    pl = plistlib.loads(plist)
    print(pl["foo"])
"""

import sys
from _typeshed import ReadableBuffer
from collections.abc import Mapping, MutableMapping
from datetime import datetime
from enum import Enum
from typing import IO, Any, Final
from typing_extensions import Self

__all__ = ["InvalidFileException", "FMT_XML", "FMT_BINARY", "load", "dump", "loads", "dumps", "UID"]

class PlistFormat(Enum):
    """An enumeration."""

    FMT_XML = 1
    FMT_BINARY = 2

FMT_XML: Final = PlistFormat.FMT_XML
FMT_BINARY: Final = PlistFormat.FMT_BINARY
if sys.version_info >= (3, 13):
    def load(
        fp: IO[bytes],
        *,
        fmt: PlistFormat | None = None,
        dict_type: type[MutableMapping[str, Any]] = ...,
        aware_datetime: bool = False,
    ) -> Any:
        """Read a .plist file. 'fp' should be a readable and binary file object.
        Return the unpacked root object (which usually is a dictionary).
        """

    def loads(
        value: ReadableBuffer | str,
        *,
        fmt: PlistFormat | None = None,
        dict_type: type[MutableMapping[str, Any]] = ...,
        aware_datetime: bool = False,
    ) -> Any:
        """Read a .plist file from a bytes object.
        Return the unpacked root object (which usually is a dictionary).
        """

else:
    def load(fp: IO[bytes], *, fmt: PlistFormat | None = None, dict_type: type[MutableMapping[str, Any]] = ...) -> Any:
        """Read a .plist file. 'fp' should be a readable and binary file object.
        Return the unpacked root object (which usually is a dictionary).
        """

    def loads(value: ReadableBuffer, *, fmt: PlistFormat | None = None, dict_type: type[MutableMapping[str, Any]] = ...) -> Any:
        """Read a .plist file from a bytes object.
        Return the unpacked root object (which usually is a dictionary).
        """

if sys.version_info >= (3, 13):
    def dump(
        value: Mapping[str, Any] | list[Any] | tuple[Any, ...] | str | bool | float | bytes | bytearray | datetime,
        fp: IO[bytes],
        *,
        fmt: PlistFormat = ...,
        sort_keys: bool = True,
        skipkeys: bool = False,
        aware_datetime: bool = False,
    ) -> None:
        """Write 'value' to a .plist file. 'fp' should be a writable,
        binary file object.
        """

    def dumps(
        value: Mapping[str, Any] | list[Any] | tuple[Any, ...] | str | bool | float | bytes | bytearray | datetime,
        *,
        fmt: PlistFormat = ...,
        skipkeys: bool = False,
        sort_keys: bool = True,
        aware_datetime: bool = False,
    ) -> bytes:
        """Return a bytes object with the contents for a .plist file."""

else:
    def dump(
        value: Mapping[str, Any] | list[Any] | tuple[Any, ...] | str | bool | float | bytes | bytearray | datetime,
        fp: IO[bytes],
        *,
        fmt: PlistFormat = ...,
        sort_keys: bool = True,
        skipkeys: bool = False,
    ) -> None:
        """Write 'value' to a .plist file. 'fp' should be a writable,
        binary file object.
        """

    def dumps(
        value: Mapping[str, Any] | list[Any] | tuple[Any, ...] | str | bool | float | bytes | bytearray | datetime,
        *,
        fmt: PlistFormat = ...,
        skipkeys: bool = False,
        sort_keys: bool = True,
    ) -> bytes:
        """Return a bytes object with the contents for a .plist file."""

class UID:
    data: int
    def __init__(self, data: int) -> None: ...
    def __index__(self) -> int: ...
    def __reduce__(self) -> tuple[type[Self], tuple[int]]: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

class InvalidFileException(ValueError):
    def __init__(self, message: str = "Invalid file") -> None: ...

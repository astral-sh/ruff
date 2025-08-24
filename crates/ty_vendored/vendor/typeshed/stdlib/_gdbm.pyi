"""This module provides an interface to the GNU DBM (GDBM) library.

This module is quite similar to the dbm module, but uses GDBM instead to
provide some additional functionality.  Please note that the file formats
created by GDBM and dbm are incompatible.

GDBM objects behave like mappings (dictionaries), except that keys and
values are always immutable bytes-like objects or strings.  Printing
a GDBM object doesn't print the keys and values, and the items() and
values() methods are not supported.
"""

import sys
from _typeshed import ReadOnlyBuffer, StrOrBytesPath
from types import TracebackType
from typing import TypeVar, overload, type_check_only
from typing_extensions import Self, TypeAlias

if sys.platform != "win32":
    _T = TypeVar("_T")
    _KeyType: TypeAlias = str | ReadOnlyBuffer
    _ValueType: TypeAlias = str | ReadOnlyBuffer

    open_flags: str

    class error(OSError): ...
    # Actual typename gdbm, not exposed by the implementation
    @type_check_only
    class _gdbm:
        def firstkey(self) -> bytes | None: ...
        def nextkey(self, key: _KeyType) -> bytes | None: ...
        def reorganize(self) -> None: ...
        def sync(self) -> None: ...
        def close(self) -> None: ...
        if sys.version_info >= (3, 13):
            def clear(self) -> None: ...

        def __getitem__(self, item: _KeyType) -> bytes: ...
        def __setitem__(self, key: _KeyType, value: _ValueType) -> None: ...
        def __delitem__(self, key: _KeyType) -> None: ...
        def __contains__(self, key: _KeyType) -> bool: ...
        def __len__(self) -> int: ...
        def __enter__(self) -> Self: ...
        def __exit__(
            self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
        ) -> None: ...
        @overload
        def get(self, k: _KeyType) -> bytes | None: ...
        @overload
        def get(self, k: _KeyType, default: _T) -> bytes | _T: ...
        def keys(self) -> list[bytes]: ...
        def setdefault(self, k: _KeyType, default: _ValueType = ...) -> bytes: ...
        # Don't exist at runtime
        __new__: None  # type: ignore[assignment]
        __init__: None  # type: ignore[assignment]

    if sys.version_info >= (3, 11):
        def open(filename: StrOrBytesPath, flags: str = "r", mode: int = 0o666, /) -> _gdbm:
            """Open a dbm database and return a dbm object.

            The filename argument is the name of the database file.

            The optional flags argument can be 'r' (to open an existing database
            for reading only -- default), 'w' (to open an existing database for
            reading and writing), 'c' (which creates the database if it doesn't
            exist), or 'n' (which always creates a new empty database).

            Some versions of gdbm support additional flags which must be
            appended to one of the flags described above.  The module constant
            'open_flags' is a string of valid additional flags.  The 'f' flag
            opens the database in fast mode; altered data will not automatically
            be written to the disk after every change.  This results in faster
            writes to the database, but may result in an inconsistent database
            if the program crashes while the database is still open.  Use the
            sync() method to force any unwritten data to be written to the disk.
            The 's' flag causes all database operations to be synchronized to
            disk.  The 'u' flag disables locking of the database file.

            The optional mode argument is the Unix mode of the file, used only
            when the database has to be created.  It defaults to octal 0o666.
            """
    else:
        def open(filename: str, flags: str = "r", mode: int = 0o666, /) -> _gdbm: ...

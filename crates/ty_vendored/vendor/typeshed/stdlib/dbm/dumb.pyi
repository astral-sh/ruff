"""A dumb and slow but simple dbm clone.

For database spam, spam.dir contains the index (a text file),
spam.bak *may* contain a backup of the index (also a text file),
while spam.dat contains the data (a binary file).

XXX TO DO:

- seems to contain a bug when updating...

- reclaim free space (currently, space once occupied by deleted or expanded
items is never reused)

- support concurrent access (currently, if two processes take turns making
updates, they can mess up the index)

- support efficient access to large databases (currently, the whole index
is read when the database is opened, and some updates rewrite the whole index)

- support opening for read-only (flag = 'm')

"""

import sys
from _typeshed import StrOrBytesPath
from collections.abc import Iterator, MutableMapping
from types import TracebackType
from typing_extensions import Self, TypeAlias

__all__ = ["error", "open"]

_KeyType: TypeAlias = str | bytes
_ValueType: TypeAlias = str | bytes

error = OSError

# This class doesn't exist at runtime. open() can return an instance of
# any of the three implementations of dbm (dumb, gnu, ndbm), and this
# class is intended to represent the common interface supported by all three.
class _Database(MutableMapping[_KeyType, bytes]):
    def __init__(self, filebasename: str, mode: str, flag: str = "c") -> None: ...
    def sync(self) -> None: ...
    def iterkeys(self) -> Iterator[bytes]: ...  # undocumented
    def close(self) -> None: ...
    def __getitem__(self, key: _KeyType) -> bytes: ...
    def __setitem__(self, key: _KeyType, val: _ValueType) -> None: ...
    def __delitem__(self, key: _KeyType) -> None: ...
    def __iter__(self) -> Iterator[bytes]: ...
    def __len__(self) -> int: ...
    def __del__(self) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> None: ...

if sys.version_info >= (3, 11):
    def open(file: StrOrBytesPath, flag: str = "c", mode: int = 0o666) -> _Database:
        """Open the database file, filename, and return corresponding object.

        The flag argument, used to control how the database is opened in the
        other DBM implementations, supports only the semantics of 'c' and 'n'
        values.  Other values will default to the semantics of 'c' value:
        the database will always opened for update and will be created if it
        does not exist.

        The optional mode argument is the UNIX mode of the file, used only when
        the database has to be created.  It defaults to octal code 0o666 (and
        will be modified by the prevailing umask).

        """

else:
    def open(file: str, flag: str = "c", mode: int = 0o666) -> _Database:
        """Open the database file, filename, and return corresponding object.

        The flag argument, used to control how the database is opened in the
        other DBM implementations, supports only the semantics of 'c' and 'n'
        values.  Other values will default to the semantics of 'c' value:
        the database will always opened for update and will be created if it
        does not exist.

        The optional mode argument is the UNIX mode of the file, used only when
        the database has to be created.  It defaults to octal code 0o666 (and
        will be modified by the prevailing umask).

        """

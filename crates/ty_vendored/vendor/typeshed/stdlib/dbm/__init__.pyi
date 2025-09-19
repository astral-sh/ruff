"""Generic interface to all dbm clones.

Use

        import dbm
        d = dbm.open(file, 'w', 0o666)

The returned object is a dbm.sqlite3, dbm.gnu, dbm.ndbm or dbm.dumb database object, dependent on the
type of database being opened (determined by the whichdb function) in the case
of an existing dbm. If the dbm does not exist and the create or new flag ('c'
or 'n') was specified, the dbm type will be determined by the availability of
the modules (tested in the above order).

It has the following interface (key and data are strings):

        d[key] = data   # store data at key (may override data at
                        # existing key)
        data = d[key]   # retrieve data at key (raise KeyError if no
                        # such key)
        del d[key]      # delete data stored at key (raises KeyError
                        # if no such key)
        flag = key in d # true if the key exists
        list = d.keys() # return a list of all existing keys (slow!)

Future versions may change the order in which implementations are
tested for existence, and add interfaces to other dbm-like
implementations.
"""

import sys
from _typeshed import StrOrBytesPath
from collections.abc import Iterator, MutableMapping
from types import TracebackType
from typing import Literal, type_check_only
from typing_extensions import Self, TypeAlias

__all__ = ["open", "whichdb", "error"]

_KeyType: TypeAlias = str | bytes
_ValueType: TypeAlias = str | bytes | bytearray
_TFlags: TypeAlias = Literal[
    "r",
    "w",
    "c",
    "n",
    "rf",
    "wf",
    "cf",
    "nf",
    "rs",
    "ws",
    "cs",
    "ns",
    "ru",
    "wu",
    "cu",
    "nu",
    "rfs",
    "wfs",
    "cfs",
    "nfs",
    "rfu",
    "wfu",
    "cfu",
    "nfu",
    "rsf",
    "wsf",
    "csf",
    "nsf",
    "rsu",
    "wsu",
    "csu",
    "nsu",
    "ruf",
    "wuf",
    "cuf",
    "nuf",
    "rus",
    "wus",
    "cus",
    "nus",
    "rfsu",
    "wfsu",
    "cfsu",
    "nfsu",
    "rfus",
    "wfus",
    "cfus",
    "nfus",
    "rsfu",
    "wsfu",
    "csfu",
    "nsfu",
    "rsuf",
    "wsuf",
    "csuf",
    "nsuf",
    "rufs",
    "wufs",
    "cufs",
    "nufs",
    "rusf",
    "wusf",
    "cusf",
    "nusf",
]

@type_check_only
class _Database(MutableMapping[_KeyType, bytes]):
    def close(self) -> None: ...
    def __getitem__(self, key: _KeyType) -> bytes: ...
    def __setitem__(self, key: _KeyType, value: _ValueType) -> None: ...
    def __delitem__(self, key: _KeyType) -> None: ...
    def __iter__(self) -> Iterator[bytes]: ...
    def __len__(self) -> int: ...
    def __del__(self) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> None: ...

# This class is not exposed. It calls itself dbm.error.
@type_check_only
class _error(Exception): ...

error: tuple[type[_error], type[OSError]]

if sys.version_info >= (3, 11):
    def whichdb(filename: StrOrBytesPath) -> str | None:
        """Guess which db package to use to open a db file.

        Return values:

        - None if the database file can't be read;
        - empty string if the file can be read but can't be recognized
        - the name of the dbm submodule (e.g. "ndbm" or "gnu") if recognized.

        Importing the given module may still fail, and opening the
        database using that module may still fail.
        """

    def open(file: StrOrBytesPath, flag: _TFlags = "r", mode: int = 0o666) -> _Database:
        """Open or create database at path given by *file*.

        Optional argument *flag* can be 'r' (default) for read-only access, 'w'
        for read-write access of an existing database, 'c' for read-write access
        to a new or existing database, and 'n' for read-write access to a new
        database.

        Note: 'r' and 'w' fail if the database doesn't exist; 'c' creates it
        only if it doesn't exist; and 'n' always creates a new database.
        """

else:
    def whichdb(filename: str) -> str | None:
        """Guess which db package to use to open a db file.

        Return values:

        - None if the database file can't be read;
        - empty string if the file can be read but can't be recognized
        - the name of the dbm submodule (e.g. "ndbm" or "gnu") if recognized.

        Importing the given module may still fail, and opening the
        database using that module may still fail.
        """

    def open(file: str, flag: _TFlags = "r", mode: int = 0o666) -> _Database:
        """Open or create database at path given by *file*.

        Optional argument *flag* can be 'r' (default) for read-only access, 'w'
        for read-write access of an existing database, 'c' for read-write access
        to a new or existing database, and 'n' for read-write access to a new
        database.

        Note: 'r' and 'w' fail if the database doesn't exist; 'c' creates it
        only if it doesn't exist; and 'n' always creates a new database.
        """

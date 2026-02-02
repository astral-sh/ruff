from _typeshed import ReadableBuffer, StrOrBytesPath, Unused
from collections.abc import Generator, MutableMapping
from typing import Final, Literal
from typing_extensions import LiteralString, Self, TypeAlias

BUILD_TABLE: Final[LiteralString]
GET_SIZE: Final[LiteralString]
LOOKUP_KEY: Final[LiteralString]
STORE_KV: Final[LiteralString]
DELETE_KEY: Final[LiteralString]
ITER_KEYS: Final[LiteralString]

_SqliteData: TypeAlias = str | ReadableBuffer | int | float

class error(OSError): ...

class _Database(MutableMapping[bytes, bytes]):
    def __init__(self, path: StrOrBytesPath, /, *, flag: Literal["r", "w", "c", "n"], mode: int) -> None: ...
    def __len__(self) -> int: ...
    def __getitem__(self, key: _SqliteData) -> bytes: ...
    def __setitem__(self, key: _SqliteData, value: _SqliteData) -> None: ...
    def __delitem__(self, key: _SqliteData) -> None: ...
    def __iter__(self) -> Generator[bytes]: ...
    def close(self) -> None: ...
    def keys(self) -> list[bytes]: ...  # type: ignore[override]
    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...

def open(filename: StrOrBytesPath, /, flag: Literal["r", "w", "c", "n"] = "r", mode: int = 0o666) -> _Database:
    """Open a dbm.sqlite3 database and return the dbm object.

    The 'filename' parameter is the name of the database file.

    The optional 'flag' parameter can be one of ...:
        'r' (default): open an existing database for read only access
        'w': open an existing database for read/write access
        'c': create a database if it does not exist; open for read/write access
        'n': always create a new, empty database; open for read/write access

    The optional 'mode' parameter is the Unix file access mode of the database;
    only used when creating a new database. Default: 0o666.
    """

import csv
import sys
from _typeshed import SupportsWrite
from collections.abc import Iterable
from typing import Any, Final, Literal, type_check_only
from typing_extensions import Self, TypeAlias

__version__: Final[str]

QUOTE_ALL: Final = 1
QUOTE_MINIMAL: Final = 0
QUOTE_NONE: Final = 3
QUOTE_NONNUMERIC: Final = 2
if sys.version_info >= (3, 12):
    QUOTE_STRINGS: Final = 4
    QUOTE_NOTNULL: Final = 5

if sys.version_info >= (3, 12):
    _QuotingType: TypeAlias = Literal[0, 1, 2, 3, 4, 5]
else:
    _QuotingType: TypeAlias = Literal[0, 1, 2, 3]

class Error(Exception): ...

_DialectLike: TypeAlias = str | Dialect | csv.Dialect | type[Dialect | csv.Dialect]

class Dialect:
    delimiter: str
    quotechar: str | None
    escapechar: str | None
    doublequote: bool
    skipinitialspace: bool
    lineterminator: str
    quoting: _QuotingType
    strict: bool
    def __new__(
        cls,
        dialect: _DialectLike | None = ...,
        delimiter: str = ",",
        doublequote: bool = True,
        escapechar: str | None = None,
        lineterminator: str = "\r\n",
        quotechar: str | None = '"',
        quoting: _QuotingType = 0,
        skipinitialspace: bool = False,
        strict: bool = False,
    ) -> Self: ...

if sys.version_info >= (3, 10):
    # This class calls itself _csv.reader.
    class Reader:
        @property
        def dialect(self) -> Dialect: ...
        line_num: int
        def __iter__(self) -> Self: ...
        def __next__(self) -> list[str]: ...

    # This class calls itself _csv.writer.
    class Writer:
        @property
        def dialect(self) -> Dialect: ...
        if sys.version_info >= (3, 13):
            def writerow(self, row: Iterable[Any], /) -> Any: ...
            def writerows(self, rows: Iterable[Iterable[Any]], /) -> None: ...
        else:
            def writerow(self, row: Iterable[Any]) -> Any: ...
            def writerows(self, rows: Iterable[Iterable[Any]]) -> None: ...

    # For the return types below.
    # These aliases can be removed when typeshed drops support for 3.9.
    _reader = Reader
    _writer = Writer
else:
    # This class is not exposed. It calls itself _csv.reader.
    @type_check_only
    class _reader:
        @property
        def dialect(self) -> Dialect: ...
        line_num: int
        def __iter__(self) -> Self: ...
        def __next__(self) -> list[str]: ...

    # This class is not exposed. It calls itself _csv.writer.
    @type_check_only
    class _writer:
        @property
        def dialect(self) -> Dialect: ...
        def writerow(self, row: Iterable[Any]) -> Any: ...
        def writerows(self, rows: Iterable[Iterable[Any]]) -> None: ...

def writer(
    csvfile: SupportsWrite[str],
    dialect: _DialectLike = "excel",
    *,
    delimiter: str = ",",
    quotechar: str | None = '"',
    escapechar: str | None = None,
    doublequote: bool = True,
    skipinitialspace: bool = False,
    lineterminator: str = "\r\n",
    quoting: _QuotingType = 0,
    strict: bool = False,
) -> _writer: ...
def reader(
    csvfile: Iterable[str],
    dialect: _DialectLike = "excel",
    *,
    delimiter: str = ",",
    quotechar: str | None = '"',
    escapechar: str | None = None,
    doublequote: bool = True,
    skipinitialspace: bool = False,
    lineterminator: str = "\r\n",
    quoting: _QuotingType = 0,
    strict: bool = False,
) -> _reader: ...
def register_dialect(
    name: str,
    dialect: type[Dialect | csv.Dialect] = ...,
    *,
    delimiter: str = ",",
    quotechar: str | None = '"',
    escapechar: str | None = None,
    doublequote: bool = True,
    skipinitialspace: bool = False,
    lineterminator: str = "\r\n",
    quoting: _QuotingType = 0,
    strict: bool = False,
) -> None: ...
def unregister_dialect(name: str) -> None: ...
def get_dialect(name: str) -> Dialect: ...
def list_dialects() -> list[str]: ...
def field_size_limit(new_limit: int = ...) -> int: ...

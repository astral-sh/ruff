"""CSV parsing and writing."""

import csv
import sys
from _typeshed import SupportsWrite
from collections.abc import Iterable
from typing import Any, Final, Literal, type_check_only
from typing_extensions import Self, TypeAlias, disjoint_base

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

@disjoint_base
class Dialect:
    """CSV dialect

    The Dialect type records CSV parsing and generation options.
    """

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
        dialect: _DialectLike | None = None,
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
    @disjoint_base
    class Reader:
        """CSV reader

        Reader objects are responsible for reading and parsing tabular data
        in CSV format.
        """

        @property
        def dialect(self) -> Dialect: ...
        line_num: int
        def __iter__(self) -> Self:
            """Implement iter(self)."""

        def __next__(self) -> list[str]:
            """Implement next(self)."""

    # This class calls itself _csv.writer.
    @disjoint_base
    class Writer:
        """CSV writer

        Writer objects are responsible for generating tabular data
        in CSV format from sequence input.
        """

        @property
        def dialect(self) -> Dialect: ...
        if sys.version_info >= (3, 13):
            def writerow(self, row: Iterable[Any], /) -> Any:
                """Construct and write a CSV record from an iterable of fields.

                Non-string elements will be converted to string.
                """

            def writerows(self, rows: Iterable[Iterable[Any]], /) -> None:
                """Construct and write a series of iterables to a csv file.

                Non-string elements will be converted to string.
                """
        else:
            def writerow(self, row: Iterable[Any]) -> Any:
                """writerow(iterable)

                Construct and write a CSV record from an iterable of fields.  Non-string
                elements will be converted to string.
                """

            def writerows(self, rows: Iterable[Iterable[Any]]) -> None:
                """writerows(iterable of iterables)

                Construct and write a series of iterables to a csv file.  Non-string
                elements will be converted to string.
                """

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
    fileobj: SupportsWrite[str],
    /,
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
) -> _writer:
    """Return a writer object that will write user data on the given file object.

    The "fileobj" argument can be any object that supports the file API.
    The optional "dialect" argument defines a CSV dialect.  The function
    also accepts optional keyword arguments which override settings
    provided by the dialect.
    """

def reader(
    iterable: Iterable[str],
    /,
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
) -> _reader:
    """Return a reader object that will process lines from the given iterable.

    The "iterable" argument can be any object that returns a line
    of input for each iteration, such as a file object or a list.  The
    optional "dialect" argument defines a CSV dialect.  The function
    also accepts optional keyword arguments which override settings
    provided by the dialect.

    The returned object is an iterator.  Each iteration returns a row
    of the CSV file (which can span multiple input lines).
    """

def register_dialect(
    name: str,
    /,
    dialect: type[Dialect | csv.Dialect] | str = "excel",
    *,
    delimiter: str = ",",
    quotechar: str | None = '"',
    escapechar: str | None = None,
    doublequote: bool = True,
    skipinitialspace: bool = False,
    lineterminator: str = "\r\n",
    quoting: _QuotingType = 0,
    strict: bool = False,
) -> None:
    """Create a mapping from a string name to a CVS dialect.

    The optional "dialect" argument specifies the base dialect instance
    or the name of the registered dialect.  The function also accepts
    optional keyword arguments which override settings provided by the
    dialect.
    """

def unregister_dialect(name: str) -> None:
    """Delete the name/dialect mapping associated with a string name."""

def get_dialect(name: str) -> Dialect:
    """Return the dialect instance associated with name."""

def list_dialects() -> list[str]:
    """Return a list of all known dialect names."""

def field_size_limit(new_limit: int = ...) -> int:
    """Sets an upper limit on parsed fields.

    Returns old limit. If limit is not given, no new limit is set and
    the old limit is returned
    """

import sys
from _csv import (
    QUOTE_ALL as QUOTE_ALL,
    QUOTE_MINIMAL as QUOTE_MINIMAL,
    QUOTE_NONE as QUOTE_NONE,
    QUOTE_NONNUMERIC as QUOTE_NONNUMERIC,
    Error as Error,
    __version__ as __version__,
    _DialectLike,
    _QuotingType,
    field_size_limit as field_size_limit,
    get_dialect as get_dialect,
    list_dialects as list_dialects,
    reader as reader,
    register_dialect as register_dialect,
    unregister_dialect as unregister_dialect,
    writer as writer,
)

if sys.version_info >= (3, 12):
    from _csv import QUOTE_NOTNULL as QUOTE_NOTNULL, QUOTE_STRINGS as QUOTE_STRINGS
if sys.version_info >= (3, 10):
    from _csv import Reader, Writer
else:
    from _csv import _reader as Reader, _writer as Writer

from _typeshed import SupportsWrite
from collections.abc import Collection, Iterable, Mapping, Sequence
from typing import Any, Generic, Literal, TypeVar, overload
from typing_extensions import Self

if sys.version_info >= (3, 12):
    from types import GenericAlias

__all__ = [
    "QUOTE_MINIMAL",
    "QUOTE_ALL",
    "QUOTE_NONNUMERIC",
    "QUOTE_NONE",
    "Error",
    "Dialect",
    "excel",
    "excel_tab",
    "field_size_limit",
    "reader",
    "writer",
    "register_dialect",
    "get_dialect",
    "list_dialects",
    "Sniffer",
    "unregister_dialect",
    "DictReader",
    "DictWriter",
    "unix_dialect",
]
if sys.version_info >= (3, 12):
    __all__ += ["QUOTE_STRINGS", "QUOTE_NOTNULL"]
if sys.version_info < (3, 13):
    __all__ += ["__doc__", "__version__"]

_T = TypeVar("_T")

class Dialect:
    delimiter: str
    quotechar: str | None
    escapechar: str | None
    doublequote: bool
    skipinitialspace: bool
    lineterminator: str
    quoting: _QuotingType
    strict: bool
    def __init__(self) -> None: ...

class excel(Dialect): ...
class excel_tab(excel): ...
class unix_dialect(Dialect): ...

class DictReader(Generic[_T]):
    fieldnames: Sequence[_T] | None
    restkey: _T | None
    restval: str | Any | None
    reader: Reader
    dialect: _DialectLike
    line_num: int
    @overload
    def __init__(
        self,
        f: Iterable[str],
        fieldnames: Sequence[_T],
        restkey: _T | None = None,
        restval: str | Any | None = None,
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
    ) -> None: ...
    @overload
    def __init__(
        self: DictReader[str],
        f: Iterable[str],
        fieldnames: Sequence[str] | None = None,
        restkey: str | None = None,
        restval: str | None = None,
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
    ) -> None: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> dict[_T | Any, str | Any]: ...
    if sys.version_info >= (3, 12):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

class DictWriter(Generic[_T]):
    fieldnames: Collection[_T]
    restval: Any | None
    extrasaction: Literal["raise", "ignore"]
    writer: Writer
    def __init__(
        self,
        f: SupportsWrite[str],
        fieldnames: Collection[_T],
        restval: Any | None = "",
        extrasaction: Literal["raise", "ignore"] = "raise",
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
    ) -> None: ...
    def writeheader(self) -> Any: ...
    def writerow(self, rowdict: Mapping[_T, Any]) -> Any: ...
    def writerows(self, rowdicts: Iterable[Mapping[_T, Any]]) -> None: ...
    if sys.version_info >= (3, 12):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

class Sniffer:
    preferred: list[str]
    def sniff(self, sample: str, delimiters: str | None = None) -> type[Dialect]: ...
    def has_header(self, sample: str) -> bool: ...

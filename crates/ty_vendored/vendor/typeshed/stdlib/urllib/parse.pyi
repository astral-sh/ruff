import sys
from collections.abc import Iterable, Mapping, Sequence
from types import GenericAlias
from typing import Any, AnyStr, Generic, Literal, NamedTuple, Protocol, overload, type_check_only
from typing_extensions import TypeAlias

__all__ = [
    "urlparse",
    "urlunparse",
    "urljoin",
    "urldefrag",
    "urlsplit",
    "urlunsplit",
    "urlencode",
    "parse_qs",
    "parse_qsl",
    "quote",
    "quote_plus",
    "quote_from_bytes",
    "unquote",
    "unquote_plus",
    "unquote_to_bytes",
    "DefragResult",
    "ParseResult",
    "SplitResult",
    "DefragResultBytes",
    "ParseResultBytes",
    "SplitResultBytes",
]

uses_relative: list[str]
uses_netloc: list[str]
uses_params: list[str]
non_hierarchical: list[str]
uses_query: list[str]
uses_fragment: list[str]
scheme_chars: str
if sys.version_info < (3, 11):
    MAX_CACHE_SIZE: int

class _ResultMixinStr:
    def encode(self, encoding: str = "ascii", errors: str = "strict") -> _ResultMixinBytes: ...

class _ResultMixinBytes:
    def decode(self, encoding: str = "ascii", errors: str = "strict") -> _ResultMixinStr: ...

class _NetlocResultMixinBase(Generic[AnyStr]):
    @property
    def username(self) -> AnyStr | None: ...
    @property
    def password(self) -> AnyStr | None: ...
    @property
    def hostname(self) -> AnyStr | None: ...
    @property
    def port(self) -> int | None: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

class _NetlocResultMixinStr(_NetlocResultMixinBase[str], _ResultMixinStr): ...
class _NetlocResultMixinBytes(_NetlocResultMixinBase[bytes], _ResultMixinBytes): ...

class _DefragResultBase(NamedTuple, Generic[AnyStr]):
    url: AnyStr
    fragment: AnyStr

class _SplitResultBase(NamedTuple, Generic[AnyStr]):
    scheme: AnyStr
    netloc: AnyStr
    path: AnyStr
    query: AnyStr
    fragment: AnyStr

class _ParseResultBase(NamedTuple, Generic[AnyStr]):
    scheme: AnyStr
    netloc: AnyStr
    path: AnyStr
    params: AnyStr
    query: AnyStr
    fragment: AnyStr

# Structured result objects for string data
class DefragResult(_DefragResultBase[str], _ResultMixinStr):
    def geturl(self) -> str: ...

class SplitResult(_SplitResultBase[str], _NetlocResultMixinStr):
    def geturl(self) -> str: ...

class ParseResult(_ParseResultBase[str], _NetlocResultMixinStr):
    def geturl(self) -> str: ...

# Structured result objects for bytes data
class DefragResultBytes(_DefragResultBase[bytes], _ResultMixinBytes):
    def geturl(self) -> bytes: ...

class SplitResultBytes(_SplitResultBase[bytes], _NetlocResultMixinBytes):
    def geturl(self) -> bytes: ...

class ParseResultBytes(_ParseResultBase[bytes], _NetlocResultMixinBytes):
    def geturl(self) -> bytes: ...

def parse_qs(
    qs: AnyStr | None,
    keep_blank_values: bool = False,
    strict_parsing: bool = False,
    encoding: str = "utf-8",
    errors: str = "replace",
    max_num_fields: int | None = None,
    separator: str = "&",
) -> dict[AnyStr, list[AnyStr]]: ...
def parse_qsl(
    qs: AnyStr | None,
    keep_blank_values: bool = False,
    strict_parsing: bool = False,
    encoding: str = "utf-8",
    errors: str = "replace",
    max_num_fields: int | None = None,
    separator: str = "&",
) -> list[tuple[AnyStr, AnyStr]]: ...
@overload
def quote(string: str, safe: str | Iterable[int] = "/", encoding: str | None = None, errors: str | None = None) -> str: ...
@overload
def quote(string: bytes | bytearray, safe: str | Iterable[int] = "/") -> str: ...
def quote_from_bytes(bs: bytes | bytearray, safe: str | Iterable[int] = "/") -> str: ...
@overload
def quote_plus(string: str, safe: str | Iterable[int] = "", encoding: str | None = None, errors: str | None = None) -> str: ...
@overload
def quote_plus(string: bytes | bytearray, safe: str | Iterable[int] = "") -> str: ...
def unquote(string: str | bytes, encoding: str = "utf-8", errors: str = "replace") -> str: ...
def unquote_to_bytes(string: str | bytes | bytearray) -> bytes: ...
def unquote_plus(string: str, encoding: str = "utf-8", errors: str = "replace") -> str: ...
@overload
def urldefrag(url: str) -> DefragResult: ...
@overload
def urldefrag(url: bytes | bytearray | None) -> DefragResultBytes: ...

# The values are passed through `str()` (unless they are bytes), so anything is valid.
_QueryType: TypeAlias = (
    Mapping[str, object]
    | Mapping[bytes, object]
    | Mapping[str | bytes, object]
    | Mapping[str, Sequence[object]]
    | Mapping[bytes, Sequence[object]]
    | Mapping[str | bytes, Sequence[object]]
    | Sequence[tuple[str | bytes, object]]
    | Sequence[tuple[str | bytes, Sequence[object]]]
)

@type_check_only
class _QuoteVia(Protocol):
    @overload
    def __call__(self, string: str, safe: str | bytes, encoding: str, errors: str, /) -> str: ...
    @overload
    def __call__(self, string: bytes, safe: str | bytes, /) -> str: ...

def urlencode(
    query: _QueryType,
    doseq: bool = False,
    safe: str | bytes = "",
    encoding: str | None = None,
    errors: str | None = None,
    quote_via: _QuoteVia = ...,
) -> str: ...
def urljoin(base: AnyStr, url: AnyStr | None, allow_fragments: bool = True) -> AnyStr: ...
@overload
def urlparse(url: str, scheme: str = "", allow_fragments: bool = True) -> ParseResult: ...
@overload
def urlparse(
    url: bytes | bytearray | None, scheme: bytes | bytearray | None | Literal[""] = "", allow_fragments: bool = True
) -> ParseResultBytes: ...
@overload
def urlsplit(url: str, scheme: str = "", allow_fragments: bool = True) -> SplitResult: ...

if sys.version_info >= (3, 11):
    @overload
    def urlsplit(
        url: bytes | None, scheme: bytes | None | Literal[""] = "", allow_fragments: bool = True
    ) -> SplitResultBytes: ...

else:
    @overload
    def urlsplit(
        url: bytes | bytearray | None, scheme: bytes | bytearray | None | Literal[""] = "", allow_fragments: bool = True
    ) -> SplitResultBytes: ...

# Requires an iterable of length 6
@overload
def urlunparse(components: Iterable[None]) -> Literal[b""]: ...  # type: ignore[overload-overlap]
@overload
def urlunparse(components: Iterable[AnyStr | None]) -> AnyStr: ...

# Requires an iterable of length 5
@overload
def urlunsplit(components: Iterable[None]) -> Literal[b""]: ...  # type: ignore[overload-overlap]
@overload
def urlunsplit(components: Iterable[AnyStr | None]) -> AnyStr: ...
def unwrap(url: str) -> str: ...

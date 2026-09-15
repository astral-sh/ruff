"""Parse (absolute and relative) URLs.

urllib.parse module is based upon the following RFC specifications.

RFC 3986 (STD66): "Uniform Resource Identifiers" by T. Berners-Lee, R. Fielding
and L.  Masinter, January 2005.

RFC 2732 : "Format for Literal IPv6 Addresses in URL's by R.Hinden, B.Carpenter
and L.Masinter, December 1999.

RFC 2396:  "Uniform Resource Identifiers (URI)": Generic Syntax by T.
Berners-Lee, R. Fielding, and L. Masinter, August 1998.

RFC 2368: "The mailto URL scheme", by P.Hoffman , L Masinter, J. Zawinski, July 1998.

RFC 1808: "Relative Uniform Resource Locators", by R. Fielding, UC Irvine, June
1995.

RFC 1738: "Uniform Resource Locators (URL)" by T. Berners-Lee, L. Masinter, M.
McCahill, December 1994

RFC 3986 is considered the current standard and any future changes to
urllib.parse module should conform with it.  The urllib.parse module is
currently not entirely compliant with this RFC due to defacto
scenarios for parsing, and for backward compatibility purposes, some
parsing quirks from older RFCs are retained. The testcases in
test_urlparse.py provides a good indicator of parsing behavior.

The WHATWG URL Parser spec should also be considered.  We are not compliant with
it either due to existing user code API behavior expectations (Hyrum's Law).
It serves as a useful guide when making changes.
"""
import sys
from collections.abc import Iterable, Mapping, Sequence
from types import GenericAlias
from typing import Any, AnyStr, Final, Generic, Literal, NamedTuple, Protocol, TypeAlias, overload, type_check_only
from typing_extensions import TypeVar

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

uses_relative: Final[list[str]]
uses_netloc: Final[list[str]]
uses_params: Final[list[str]]
non_hierarchical: Final[list[str]]
uses_query: Final[list[str]]
uses_fragment: Final[list[str]]
scheme_chars: Final[str]
if sys.version_info < (3, 11):
    MAX_CACHE_SIZE: Final[int]

_ResultStrT = TypeVar("_ResultStrT", str, bytes)
_ResultComponentT = TypeVar("_ResultComponentT", str, bytes, str | None, bytes | None)
_StrComponentT = TypeVar("_StrComponentT", str, str | None, default=str)
_BytesComponentT = TypeVar("_BytesComponentT", bytes, bytes | None, default=bytes)

class _ResultMixinStr:
    """Standard approach to encoding parsed results from str to bytes"""
    __slots__ = ()
    def encode(self, encoding: str = "ascii", errors: str = "strict") -> _ResultMixinBytes: ...

class _ResultMixinBytes:
    """Standard approach to decoding parsed results from bytes to str"""
    __slots__ = ()
    def decode(self, encoding: str = "ascii", errors: str = "strict") -> _ResultMixinStr: ...

class _NetlocResultMixinBase(Generic[AnyStr]):
    """Shared methods for the parsed result objects containing a netloc element"""
    __slots__ = ()
    @property
    def username(self) -> AnyStr | None: ...
    @property
    def password(self) -> AnyStr | None: ...
    @property
    def hostname(self) -> AnyStr | None: ...
    @property
    def port(self) -> int | None: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

For example, for t = list[int], t.__origin__ is list and t.__args__
is (int,).
"""

class _NetlocResultMixinStr(_NetlocResultMixinBase[str], _ResultMixinStr):
    __slots__ = ()

class _NetlocResultMixinBytes(_NetlocResultMixinBase[bytes], _ResultMixinBytes):
    __slots__ = ()

# Need to duplicate the whole class because mypy rejects version-specific
# branches in namedtuple bodies.
if sys.version_info >= (3, 15):
    class _DefragResultBase(NamedTuple, Generic[_ResultStrT, _ResultComponentT]):
        """
DefragResult(url, fragment)

A 2-tuple that contains the url without fragment identifier and the fragment
identifier as a separate argument.
"""
        url: _ResultStrT
        fragment: _ResultComponentT
        # Ignore needed due to mypy#21453.
        def geturl(self) -> _ResultStrT: ...  # type: ignore[misc]

else:
    class _DefragResultBase(NamedTuple, Generic[_ResultStrT, _ResultComponentT]):
        """
DefragResult(url, fragment)

A 2-tuple that contains the url without fragment identifier and the fragment
identifier as a separate argument.
"""
        url: _ResultStrT
        fragment: _ResultComponentT

if sys.version_info >= (3, 15):
    class _SplitResultBase(NamedTuple, Generic[_ResultStrT, _ResultComponentT]):
        """
SplitResult(scheme, netloc, path, query, fragment)

A 5-tuple that contains the different components of a URL. Similar to
ParseResult, but does not split params.
"""
        scheme: _ResultComponentT
        netloc: _ResultComponentT
        path: _ResultStrT
        query: _ResultComponentT
        fragment: _ResultComponentT
        # Ignore needed due to mypy#21453.
        def geturl(self) -> _ResultStrT: ...  # type: ignore[misc]

else:
    class _SplitResultBase(NamedTuple, Generic[_ResultStrT, _ResultComponentT]):
        """
SplitResult(scheme, netloc, path, query, fragment)

A 5-tuple that contains the different components of a URL. Similar to
ParseResult, but does not split params.
"""
        scheme: _ResultComponentT
        netloc: _ResultComponentT
        path: _ResultStrT
        query: _ResultComponentT
        fragment: _ResultComponentT

if sys.version_info >= (3, 15):
    class _ParseResultBase(NamedTuple, Generic[_ResultStrT, _ResultComponentT]):
        """
ParseResult(scheme, netloc, path, params, query, fragment)

A 6-tuple that contains components of a parsed URL.
"""
        scheme: _ResultComponentT
        netloc: _ResultComponentT
        path: _ResultStrT
        params: _ResultComponentT
        query: _ResultComponentT
        fragment: _ResultComponentT
        # Ignore needed due to mypy#21453.
        def geturl(self) -> _ResultStrT: ...  # type: ignore[misc]

else:
    class _ParseResultBase(NamedTuple, Generic[_ResultStrT, _ResultComponentT]):
        """
ParseResult(scheme, netloc, path, params, query, fragment)

A 6-tuple that contains components of a parsed URL.
"""
        scheme: _ResultComponentT
        netloc: _ResultComponentT
        path: _ResultStrT
        params: _ResultComponentT
        query: _ResultComponentT
        fragment: _ResultComponentT

if sys.version_info >= (3, 15):
    # Structured result objects for string data
    class DefragResult(_DefragResultBase[str, _StrComponentT], _ResultMixinStr, Generic[_StrComponentT]): ...
    class SplitResult(_SplitResultBase[str, _StrComponentT], _NetlocResultMixinStr, Generic[_StrComponentT]): ...
    class ParseResult(_ParseResultBase[str, _StrComponentT], _NetlocResultMixinStr, Generic[_StrComponentT]): ...
    # Structured result objects for bytes data
    class DefragResultBytes(_DefragResultBase[bytes, _BytesComponentT], _ResultMixinBytes, Generic[_BytesComponentT]): ...
    class SplitResultBytes(_SplitResultBase[bytes, _BytesComponentT], _NetlocResultMixinBytes, Generic[_BytesComponentT]): ...
    class ParseResultBytes(_ParseResultBase[bytes, _BytesComponentT], _NetlocResultMixinBytes, Generic[_BytesComponentT]): ...

else:
    # Structured result objects for string data
    class DefragResult(_DefragResultBase[str, str], _ResultMixinStr):
        def geturl(self) -> str: ...

    class SplitResult(_SplitResultBase[str, str], _NetlocResultMixinStr):
        def geturl(self) -> str: ...

    class ParseResult(_ParseResultBase[str, str], _NetlocResultMixinStr):
        def geturl(self) -> str: ...

    # Structured result objects for bytes data
    class DefragResultBytes(_DefragResultBase[bytes, bytes], _ResultMixinBytes):
        def geturl(self) -> bytes: ...

    class SplitResultBytes(_SplitResultBase[bytes, bytes], _NetlocResultMixinBytes):
        def geturl(self) -> bytes: ...

    class ParseResultBytes(_ParseResultBase[bytes, bytes], _NetlocResultMixinBytes):
        def geturl(self) -> bytes: ...

def parse_qs(
    qs: AnyStr | None,
    keep_blank_values: bool = False,
    strict_parsing: bool = False,
    encoding: str = "utf-8",
    errors: str = "replace",
    max_num_fields: int | None = None,
    separator: str = "&",
) -> dict[AnyStr, list[AnyStr]]:
    """Parse a query given as a string argument.

Arguments:

qs: percent-encoded query string to be parsed

keep_blank_values: flag indicating whether blank values in
    percent-encoded queries should be treated as blank strings.
    A true value indicates that blanks should be retained as
    blank strings.  The default false value indicates that
    blank values are to be ignored and treated as if they were
    not included.

strict_parsing: flag indicating what to do with parsing errors.
    If false (the default), errors are silently ignored.
    If true, errors raise a ValueError exception.

encoding and errors: specify how to decode percent-encoded sequences
    into Unicode characters, as accepted by the bytes.decode() method.

max_num_fields: int. If set, then throws a ValueError if there
    are more than n fields read by parse_qsl().

separator: str. The symbol to use for separating the query arguments.
    Defaults to &.

Returns a dictionary.
"""
def parse_qsl(
    qs: AnyStr | None,
    keep_blank_values: bool = False,
    strict_parsing: bool = False,
    encoding: str = "utf-8",
    errors: str = "replace",
    max_num_fields: int | None = None,
    separator: str = "&",
) -> list[tuple[AnyStr, AnyStr]]:
    """Parse a query given as a string argument.

Arguments:

qs: percent-encoded query string to be parsed

keep_blank_values: flag indicating whether blank values in
    percent-encoded queries should be treated as blank strings.
    A true value indicates that blanks should be retained as blank
    strings.  The default false value indicates that blank values
    are to be ignored and treated as if they were  not included.

strict_parsing: flag indicating what to do with parsing errors. If
    false (the default), errors are silently ignored. If true,
    errors raise a ValueError exception.

encoding and errors: specify how to decode percent-encoded sequences
    into Unicode characters, as accepted by the bytes.decode() method.

max_num_fields: int. If set, then throws a ValueError
    if there are more than n fields read by parse_qsl().

separator: str. The symbol to use for separating the query arguments.
    Defaults to &.

Returns a list, as G-d intended.
"""

@overload
def quote(string: str, safe: str | Iterable[int] = "/", encoding: str | None = None, errors: str | None = None) -> str:
    """quote('abc def') -> 'abc%20def'

Each part of a URL, e.g. the path info, the query, etc., has a
different set of reserved characters that must be quoted. The
quote function offers a cautious (not minimal) way to quote a
string for most of these parts.

RFC 3986 Uniform Resource Identifier (URI): Generic Syntax lists
the following (un)reserved characters.

unreserved    = ALPHA / DIGIT / "-" / "." / "_" / "~"
reserved      = gen-delims / sub-delims
gen-delims    = ":" / "/" / "?" / "#" / "[" / "]" / "@"
sub-delims    = "!" / "$" / "&" / "'" / "(" / ")"
              / "*" / "+" / "," / ";" / "="

Each of the reserved characters is reserved in some component of a URL,
but not necessarily in all of them.

The quote function %-escapes all characters that are neither in the
unreserved chars ("always safe") nor the additional chars set via the
safe arg.

The default for the safe arg is '/'. The character is reserved, but in
typical usage the quote function is being called on a path where the
existing slash characters are to be preserved.

Python 3.7 updates from using RFC 2396 to RFC 3986 to quote URL strings.
Now, "~" is included in the set of unreserved characters.

string and safe may be either str or bytes objects. encoding and errors
must not be specified if string is a bytes object.

The optional encoding and errors parameters specify how to deal with
non-ASCII characters, as accepted by the str.encode method.
By default, encoding='utf-8' (characters are encoded with UTF-8), and
errors='strict' (unsupported characters raise a UnicodeEncodeError).
"""
@overload
def quote(string: bytes | bytearray, safe: str | Iterable[int] = "/") -> str: ...

def quote_from_bytes(bs: bytes | bytearray, safe: str | Iterable[int] = "/") -> str:
    """Like quote(), but accepts a bytes object rather than a str, and does
not perform string-to-bytes encoding.  It always returns an ASCII string.
quote_from_bytes(b'abc def?') -> 'abc%20def%3f'
"""

@overload
def quote_plus(string: str, safe: str | Iterable[int] = "", encoding: str | None = None, errors: str | None = None) -> str:
    """Like quote(), but also replace ' ' with '+', as required for quoting
HTML form values. Plus signs in the original string are escaped unless
they are included in safe. It also does not have safe default to '/'.
"""
@overload
def quote_plus(string: bytes | bytearray, safe: str | Iterable[int] = "") -> str: ...

def unquote(string: str | bytes, encoding: str = "utf-8", errors: str = "replace") -> str:
    """Replace %xx escapes by their single-character equivalent. The optional
encoding and errors parameters specify how to decode percent-encoded
sequences into Unicode characters, as accepted by the bytes.decode()
method.
By default, percent-encoded sequences are decoded with UTF-8, and invalid
sequences are replaced by a placeholder character.

unquote('abc%20def') -> 'abc def'.
"""
def unquote_to_bytes(string: str | bytes | bytearray) -> bytes:
    """unquote_to_bytes('abc%20def') -> b'abc def'."""
def unquote_plus(string: str, encoding: str = "utf-8", errors: str = "replace") -> str:
    """Like unquote(), but also replace plus signs by spaces, as required for
unquoting HTML form values.

unquote_plus('%7e/abc+def') -> '~/abc def'
"""

@overload
def urldefrag(url: str) -> DefragResult:
    """Removes any existing fragment from URL.

Returns a tuple of the defragmented URL and the fragment.  If
the URL contained no fragments, the second element is the
empty string or None if missing_as_none is True.
"""
@overload
def urldefrag(url: bytes | bytearray | None) -> DefragResultBytes: ...
if sys.version_info >= (3, 15):
    @overload
    def urldefrag(url: str, *, missing_as_none: Literal[True]) -> DefragResult[str | None]:
        """Removes any existing fragment from URL.

Returns a tuple of the defragmented URL and the fragment.  If
the URL contained no fragments, the second element is the
empty string or None if missing_as_none is True.
"""
    @overload
    def urldefrag(url: str, *, missing_as_none: Literal[False] = False) -> DefragResult[str]: ...
    @overload
    def urldefrag(url: bytes | bytearray | None, *, missing_as_none: Literal[True]) -> DefragResultBytes[bytes | None]: ...
    @overload
    def urldefrag(url: bytes | bytearray | None, *, missing_as_none: Literal[False] = False) -> DefragResultBytes[bytes]: ...
    @overload
    def urldefrag(url: str, *, missing_as_none: bool) -> DefragResult[str | None]: ...
    @overload
    def urldefrag(url: bytes | bytearray | None, *, missing_as_none: bool) -> DefragResultBytes[bytes | None]: ...

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
) -> str:
    """Encode a dict or sequence of two-element tuples into a URL query string.

If any values in the query arg are sequences and doseq is true, each
sequence element is converted to a separate parameter.

If the query arg is a sequence of two-element tuples, the order of the
parameters in the output will match the order of parameters in the
input.

The components of a query arg may each be either a string or a bytes type.

The safe, encoding, and errors parameters are passed down to the function
specified by quote_via (encoding and errors only if a component is a str).
"""
def urljoin(base: AnyStr, url: AnyStr | None, allow_fragments: bool = True) -> AnyStr:
    """Join a base URL and a possibly relative URL to form an absolute
interpretation of the latter.
"""

@overload
def urlparse(url: str, scheme: str = "", allow_fragments: bool = True) -> ParseResult:
    """Parse a URL into 6 components:
<scheme>://<netloc>/<path>;<params>?<query>#<fragment>

The result is a named 6-tuple with fields corresponding to the
above. It is either a ParseResult or ParseResultBytes object,
depending on the type of the url parameter.

The username, password, hostname, and port sub-components of netloc
can also be accessed as attributes of the returned object.

The scheme argument provides the default value of the scheme
component when no scheme is found in url.

If allow_fragments is False, no attempt is made to separate the
fragment component from the previous component, which can be either
path or query.

Note that % escapes are not expanded.

urlsplit() should generally be used instead of urlparse().
"""
@overload
def urlparse(
    url: bytes | bytearray | None, scheme: bytes | bytearray | None | Literal[""] = "", allow_fragments: bool = True
) -> ParseResultBytes: ...
if sys.version_info >= (3, 15):
    @overload
    def urlparse(
        url: str, scheme: str = "", allow_fragments: bool = True, *, missing_as_none: Literal[True]
    ) -> ParseResult[str | None]:
        """Parse a URL into 6 components:
<scheme>://<netloc>/<path>;<params>?<query>#<fragment>

The result is a named 6-tuple with fields corresponding to the
above. It is either a ParseResult or ParseResultBytes object,
depending on the type of the url parameter.

The username, password, hostname, and port sub-components of netloc
can also be accessed as attributes of the returned object.

The scheme argument provides the default value of the scheme
component when no scheme is found in url.

If allow_fragments is False, no attempt is made to separate the
fragment component from the previous component, which can be either
path or query.

Note that % escapes are not expanded.

urlsplit() should generally be used instead of urlparse().
"""
    @overload
    def urlparse(
        url: str, scheme: str = "", allow_fragments: bool = True, *, missing_as_none: Literal[False] = False
    ) -> ParseResult[str]: ...
    @overload
    def urlparse(
        url: bytes | bytearray | None,
        scheme: bytes | bytearray | None | Literal[""] = "",
        allow_fragments: bool = True,
        *,
        missing_as_none: Literal[True],
    ) -> ParseResultBytes[bytes | None]: ...
    @overload
    def urlparse(
        url: bytes | bytearray | None,
        scheme: bytes | bytearray | None | Literal[""] = "",
        allow_fragments: bool = True,
        *,
        missing_as_none: Literal[False] = False,
    ) -> ParseResultBytes[bytes]: ...
    @overload
    def urlparse(
        url: str, scheme: str = "", allow_fragments: bool = True, *, missing_as_none: bool
    ) -> ParseResult[str | None]: ...
    @overload
    def urlparse(
        url: bytes | bytearray | None,
        scheme: bytes | bytearray | None | Literal[""] = "",
        allow_fragments: bool = True,
        *,
        missing_as_none: bool,
    ) -> ParseResultBytes[bytes | None]: ...

@overload
def urlsplit(url: str, scheme: str = "", allow_fragments: bool = True) -> SplitResult:
    """Parse a URL into 5 components:
<scheme>://<netloc>/<path>?<query>#<fragment>

The result is a named 5-tuple with fields corresponding to the
above. It is either a SplitResult or SplitResultBytes object,
depending on the type of the url parameter.

The username, password, hostname, and port sub-components of netloc
can also be accessed as attributes of the returned object.

The scheme argument provides the default value of the scheme
component when no scheme is found in url.

If allow_fragments is False, no attempt is made to separate the
fragment component from the previous component, which can be either
path or query.

Note that % escapes are not expanded.
"""

if sys.version_info >= (3, 11):
    @overload
    def urlsplit(
        url: bytes | None, scheme: bytes | None | Literal[""] = "", allow_fragments: bool = True
    ) -> SplitResultBytes:
        """Parse a URL into 5 components:
<scheme>://<netloc>/<path>?<query>#<fragment>

The result is a named 5-tuple with fields corresponding to the
above. It is either a SplitResult or SplitResultBytes object,
depending on the type of the url parameter.

The username, password, hostname, and port sub-components of netloc
can also be accessed as attributes of the returned object.

The scheme argument provides the default value of the scheme
component when no scheme is found in url.

If allow_fragments is False, no attempt is made to separate the
fragment component from the previous component, which can be either
path or query.

Note that % escapes are not expanded.
"""
else:
    @overload
    def urlsplit(
        url: bytes | bytearray | None, scheme: bytes | bytearray | None | Literal[""] = "", allow_fragments: bool = True
    ) -> SplitResultBytes:
        """Parse a URL into 5 components:
    <scheme>://<netloc>/<path>?<query>#<fragment>

    The result is a named 5-tuple with fields corresponding to the
    above. It is either a SplitResult or SplitResultBytes object,
    depending on the type of the url parameter.

    The username, password, hostname, and port sub-components of netloc
    can also be accessed as attributes of the returned object.

    The scheme argument provides the default value of the scheme
    component when no scheme is found in url.

    If allow_fragments is False, no attempt is made to separate the
    fragment component from the previous component, which can be either
    path or query.

    Note that % escapes are not expanded.
    """
if sys.version_info >= (3, 15):
    @overload
    def urlsplit(
        url: str, scheme: str = "", allow_fragments: bool = True, *, missing_as_none: Literal[True]
    ) -> SplitResult[str | None]:
        """Parse a URL into 5 components:
<scheme>://<netloc>/<path>?<query>#<fragment>

The result is a named 5-tuple with fields corresponding to the
above. It is either a SplitResult or SplitResultBytes object,
depending on the type of the url parameter.

The username, password, hostname, and port sub-components of netloc
can also be accessed as attributes of the returned object.

The scheme argument provides the default value of the scheme
component when no scheme is found in url.

If allow_fragments is False, no attempt is made to separate the
fragment component from the previous component, which can be either
path or query.

Note that % escapes are not expanded.
"""
    @overload
    def urlsplit(
        url: str, scheme: str = "", allow_fragments: bool = True, *, missing_as_none: Literal[False] = False
    ) -> SplitResult[str]: ...
    @overload
    def urlsplit(
        url: bytes | None,
        scheme: bytes | None | Literal[""] = "",
        allow_fragments: bool = True,
        *,
        missing_as_none: Literal[True],
    ) -> SplitResultBytes[bytes | None]: ...
    @overload
    def urlsplit(
        url: bytes | None,
        scheme: bytes | None | Literal[""] = "",
        allow_fragments: bool = True,
        *,
        missing_as_none: Literal[False] = False,
    ) -> SplitResultBytes[bytes]: ...
    @overload
    def urlsplit(
        url: str, scheme: str = "", allow_fragments: bool = True, *, missing_as_none: bool
    ) -> SplitResult[str | None]: ...
    @overload
    def urlsplit(
        url: bytes | None, scheme: bytes | None | Literal[""] = "", allow_fragments: bool = True, *, missing_as_none: bool
    ) -> SplitResultBytes[bytes | None]: ...

if sys.version_info >= (3, 15):
    # Requires an iterable of length 6
    @overload
    def urlunparse(components: Iterable[None], *, keep_empty: bool = ...) -> Literal[b""]:  # type: ignore[overload-overlap]
        """Put a parsed URL back together again.  This may result in a
slightly different, but equivalent URL, if the URL that was parsed
originally had redundant delimiters, e.g. a ? with an empty query
(the draft states that these are equivalent) and keep_empty is false
or components is the result of the urlparse() call with
missing_as_none=False.
"""
    @overload
    def urlunparse(components: Iterable[AnyStr | None], *, keep_empty: bool = ...) -> AnyStr: ...
else:
    # Requires an iterable of length 6
    @overload
    def urlunparse(components: Iterable[None]) -> Literal[b""]:  # type: ignore[overload-overlap]
        """Put a parsed URL back together again.  This may result in a
slightly different, but equivalent URL, if the URL that was parsed
originally had redundant delimiters, e.g. a ? with an empty query
(the draft states that these are equivalent).
"""
    @overload
    def urlunparse(components: Iterable[AnyStr | None]) -> AnyStr: ...

if sys.version_info >= (3, 15):
    # Requires an iterable of length 5
    @overload
    def urlunsplit(components: Iterable[None], *, keep_empty: bool = ...) -> Literal[b""]:  # type: ignore[overload-overlap]
        """Combine the elements of a tuple as returned by urlsplit() into a
complete URL as a string. The data argument can be any five-item iterable.
This may result in a slightly different, but equivalent URL, if the URL that
was parsed originally had unnecessary delimiters (for example, a ? with an
empty query; the RFC states that these are equivalent) and keep_empty
is false or components is the result of the urlsplit() call with
missing_as_none=False.
"""
    @overload
    def urlunsplit(components: Iterable[AnyStr | None], *, keep_empty: bool = ...) -> AnyStr: ...
else:
    # Requires an iterable of length 5
    @overload
    def urlunsplit(components: Iterable[None]) -> Literal[b""]:  # type: ignore[overload-overlap]
        """Combine the elements of a tuple as returned by urlsplit() into a
complete URL as a string. The data argument can be any five-item iterable.
This may result in a slightly different, but equivalent URL, if the URL that
was parsed originally had unnecessary delimiters (for example, a ? with an
empty query; the RFC states that these are equivalent).
"""
    @overload
    def urlunsplit(components: Iterable[AnyStr | None]) -> AnyStr: ...

def unwrap(url: str) -> str:
    """Transform a string like '<URL:scheme://host/path>' into 'scheme://host/path'.

The string is returned unchanged if it's not a wrapped URL.
"""

"""Header encoding and decoding functionality."""

from collections.abc import Iterable
from email.charset import Charset
from typing import Any, ClassVar

__all__ = ["Header", "decode_header", "make_header"]

class Header:
    def __init__(
        self,
        s: bytes | bytearray | str | None = None,
        charset: Charset | str | None = None,
        maxlinelen: int | None = None,
        header_name: str | None = None,
        continuation_ws: str = " ",
        errors: str = "strict",
    ) -> None:
        """Create a MIME-compliant header that can contain many character sets.

        Optional s is the initial header value.  If None, the initial header
        value is not set.  You can later append to the header with .append()
        method calls.  s may be a byte string or a Unicode string, but see the
        .append() documentation for semantics.

        Optional charset serves two purposes: it has the same meaning as the
        charset argument to the .append() method.  It also sets the default
        character set for all subsequent .append() calls that omit the charset
        argument.  If charset is not provided in the constructor, the us-ascii
        charset is used both as s's initial charset and as the default for
        subsequent .append() calls.

        The maximum line length can be specified explicitly via maxlinelen. For
        splitting the first line to a shorter value (to account for the field
        header which isn't included in s, e.g. 'Subject') pass in the name of
        the field in header_name.  The default maxlinelen is 78 as recommended
        by RFC 2822.

        continuation_ws must be RFC 2822 compliant folding whitespace (usually
        either a space or a hard tab) which will be prepended to continuation
        lines.

        errors is passed through to the .append() call.
        """

    def append(self, s: bytes | bytearray | str, charset: Charset | str | None = None, errors: str = "strict") -> None:
        """Append a string to the MIME header.

        Optional charset, if given, should be a Charset instance or the name
        of a character set (which will be converted to a Charset instance).  A
        value of None (the default) means that the charset given in the
        constructor is used.

        s may be a byte string or a Unicode string.  If it is a byte string
        (i.e. isinstance(s, str) is false), then charset is the encoding of
        that byte string, and a UnicodeError will be raised if the string
        cannot be decoded with that charset.  If s is a Unicode string, then
        charset is a hint specifying the character set of the characters in
        the string.  In either case, when producing an RFC 2822 compliant
        header using RFC 2047 rules, the string will be encoded using the
        output codec of the charset.  If the string cannot be encoded to the
        output codec, a UnicodeError will be raised.

        Optional 'errors' is passed as the errors argument to the decode
        call if s is a byte string.
        """

    def encode(self, splitchars: str = ";, \t", maxlinelen: int | None = None, linesep: str = "\n") -> str:
        """Encode a message header into an RFC-compliant format.

        There are many issues involved in converting a given string for use in
        an email header.  Only certain character sets are readable in most
        email clients, and as header strings can only contain a subset of
        7-bit ASCII, care must be taken to properly convert and encode (with
        Base64 or quoted-printable) header strings.  In addition, there is a
        75-character length limit on any given encoded header field, so
        line-wrapping must be performed, even with double-byte character sets.

        Optional maxlinelen specifies the maximum length of each generated
        line, exclusive of the linesep string.  Individual lines may be longer
        than maxlinelen if a folding point cannot be found.  The first line
        will be shorter by the length of the header name plus ": " if a header
        name was specified at Header construction time.  The default value for
        maxlinelen is determined at header construction time.

        Optional splitchars is a string containing characters which should be
        given extra weight by the splitting algorithm during normal header
        wrapping.  This is in very rough support of RFC 2822's 'higher level
        syntactic breaks':  split points preceded by a splitchar are preferred
        during line splitting, with the characters preferred in the order in
        which they appear in the string.  Space and tab may be included in the
        string to indicate whether preference should be given to one over the
        other as a split point when other split chars do not appear in the line
        being split.  Splitchars does not affect RFC 2047 encoded lines.

        Optional linesep is a string to be used to separate the lines of
        the value.  The default value is the most useful for typical
        Python applications, but it can be set to \\r\\n to produce RFC-compliant
        line separators when needed.
        """
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __eq__(self, other: object) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...

# decode_header() either returns list[tuple[str, None]] if the header
# contains no encoded parts, or list[tuple[bytes, str | None]] if the header
# contains at least one encoded part.
def decode_header(header: Header | str) -> list[tuple[Any, Any | None]]:
    """Decode a message header value without converting charset.

    For historical reasons, this function may return either:

    1. A list of length 1 containing a pair (str, None).
    2. A list of (bytes, charset) pairs containing each of the decoded
       parts of the header.  Charset is None for non-encoded parts of the header,
       otherwise a lower-case string containing the name of the character set
       specified in the encoded string.

    header may be a string that may or may not contain RFC2047 encoded words,
    or it may be a Header object.

    An email.errors.HeaderParseError may be raised when certain decoding error
    occurs (e.g. a base64 decoding exception).

    This function exists for backwards compatibility only. For new code, we
    recommend using email.headerregistry.HeaderRegistry instead.
    """

def make_header(
    decoded_seq: Iterable[tuple[bytes | bytearray | str, str | None]],
    maxlinelen: int | None = None,
    header_name: str | None = None,
    continuation_ws: str = " ",
) -> Header:
    """Create a Header from a sequence of pairs as returned by decode_header()

    decode_header() takes a header value string and returns a sequence of
    pairs of the format (decoded_string, charset) where charset is the string
    name of the character set.

    This function takes one of those sequence of pairs and returns a Header
    instance.  Optional maxlinelen, header_name, and continuation_ws are as in
    the Header constructor.

    This function exists for backwards compatibility only, and is not
    recommended for use in new code.
    """

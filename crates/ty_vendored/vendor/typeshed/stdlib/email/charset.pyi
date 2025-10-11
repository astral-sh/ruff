from collections.abc import Callable, Iterator
from email.message import Message
from typing import ClassVar, Final, overload

__all__ = ["Charset", "add_alias", "add_charset", "add_codec"]

QP: Final = 1  # undocumented
BASE64: Final = 2  # undocumented
SHORTEST: Final = 3  # undocumented
RFC2047_CHROME_LEN: Final = 7  # undocumented
DEFAULT_CHARSET: Final = "us-ascii"  # undocumented
UNKNOWN8BIT: Final = "unknown-8bit"  # undocumented
EMPTYSTRING: Final = ""  # undocumented
CHARSETS: Final[dict[str, tuple[int | None, int | None, str | None]]]
ALIASES: Final[dict[str, str]]
CODEC_MAP: Final[dict[str, str | None]]  # undocumented

class Charset:
    """Map character sets to their email properties.

    This class provides information about the requirements imposed on email
    for a specific character set.  It also provides convenience routines for
    converting between character sets, given the availability of the
    applicable codecs.  Given a character set, it will do its best to provide
    information on how to use that character set in an email in an
    RFC-compliant way.

    Certain character sets must be encoded with quoted-printable or base64
    when used in email headers or bodies.  Certain character sets must be
    converted outright, and are not allowed in email.  Instances of this
    module expose the following information about a character set:

    input_charset: The initial character set specified.  Common aliases
                   are converted to their 'official' email names (e.g. latin_1
                   is converted to iso-8859-1).  Defaults to 7-bit us-ascii.

    header_encoding: If the character set must be encoded before it can be
                     used in an email header, this attribute will be set to
                     charset.QP (for quoted-printable), charset.BASE64 (for
                     base64 encoding), or charset.SHORTEST for the shortest of
                     QP or BASE64 encoding.  Otherwise, it will be None.

    body_encoding: Same as header_encoding, but describes the encoding for the
                   mail message's body, which indeed may be different than the
                   header encoding.  charset.SHORTEST is not allowed for
                   body_encoding.

    output_charset: Some character sets must be converted before they can be
                    used in email headers or bodies.  If the input_charset is
                    one of them, this attribute will contain the name of the
                    charset output will be converted to.  Otherwise, it will
                    be None.

    input_codec: The name of the Python codec used to convert the
                 input_charset to Unicode.  If no conversion codec is
                 necessary, this attribute will be None.

    output_codec: The name of the Python codec used to convert Unicode
                  to the output_charset.  If no conversion codec is necessary,
                  this attribute will have the same value as the input_codec.
    """

    input_charset: str
    header_encoding: int
    body_encoding: int
    output_charset: str | None
    input_codec: str | None
    output_codec: str | None
    def __init__(self, input_charset: str = "us-ascii") -> None: ...
    def get_body_encoding(self) -> str | Callable[[Message], None]:
        """Return the content-transfer-encoding used for body encoding.

        This is either the string 'quoted-printable' or 'base64' depending on
        the encoding used, or it is a function in which case you should call
        the function with a single argument, the Message object being
        encoded.  The function should then set the Content-Transfer-Encoding
        header itself to whatever is appropriate.

        Returns "quoted-printable" if self.body_encoding is QP.
        Returns "base64" if self.body_encoding is BASE64.
        Returns conversion function otherwise.
        """

    def get_output_charset(self) -> str | None:
        """Return the output character set.

        This is self.output_charset if that is not None, otherwise it is
        self.input_charset.
        """

    def header_encode(self, string: str) -> str:
        """Header-encode a string by converting it first to bytes.

        The type of encoding (base64 or quoted-printable) will be based on
        this charset's `header_encoding`.

        :param string: A unicode string for the header.  It must be possible
            to encode this string to bytes using the character set's
            output codec.
        :return: The encoded string, with RFC 2047 chrome.
        """

    def header_encode_lines(self, string: str, maxlengths: Iterator[int]) -> list[str | None]:
        """Header-encode a string by converting it first to bytes.

        This is similar to `header_encode()` except that the string is fit
        into maximum line lengths as given by the argument.

        :param string: A unicode string for the header.  It must be possible
            to encode this string to bytes using the character set's
            output codec.
        :param maxlengths: Maximum line length iterator.  Each element
            returned from this iterator will provide the next maximum line
            length.  This parameter is used as an argument to built-in next()
            and should never be exhausted.  The maximum line lengths should
            not count the RFC 2047 chrome.  These line lengths are only a
            hint; the splitter does the best it can.
        :return: Lines of encoded strings, each with RFC 2047 chrome.
        """

    @overload
    def body_encode(self, string: None) -> None:
        """Body-encode a string by converting it first to bytes.

        The type of encoding (base64 or quoted-printable) will be based on
        self.body_encoding.  If body_encoding is None, we assume the
        output charset is a 7bit encoding, so re-encoding the decoded
        string using the ascii codec produces the correct string version
        of the content.
        """

    @overload
    def body_encode(self, string: str | bytes) -> str: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __eq__(self, other: object) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...

def add_charset(
    charset: str, header_enc: int | None = None, body_enc: int | None = None, output_charset: str | None = None
) -> None:
    """Add character set properties to the global registry.

    charset is the input character set, and must be the canonical name of a
    character set.

    Optional header_enc and body_enc is either charset.QP for
    quoted-printable, charset.BASE64 for base64 encoding, charset.SHORTEST for
    the shortest of qp or base64 encoding, or None for no encoding.  SHORTEST
    is only valid for header_enc.  It describes how message headers and
    message bodies in the input charset are to be encoded.  Default is no
    encoding.

    Optional output_charset is the character set that the output should be
    in.  Conversions will proceed from input charset, to Unicode, to the
    output charset when the method Charset.convert() is called.  The default
    is to output in the same character set as the input.

    Both input_charset and output_charset must have Unicode codec entries in
    the module's charset-to-codec mapping; use add_codec(charset, codecname)
    to add codecs the module does not know about.  See the codecs module's
    documentation for more information.
    """

def add_alias(alias: str, canonical: str) -> None:
    """Add a character set alias.

    alias is the alias name, e.g. latin-1
    canonical is the character set's canonical name, e.g. iso-8859-1
    """

def add_codec(charset: str, codecname: str) -> None:
    """Add a codec that map characters in the given charset to/from Unicode.

    charset is the canonical name of a character set.  codecname is the name
    of a Python codec, as appropriate for the second argument to the unicode()
    built-in, or to the encode() method of a Unicode string.
    """

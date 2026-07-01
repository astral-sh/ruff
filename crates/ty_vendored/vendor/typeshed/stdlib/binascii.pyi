"""Conversion between binary data and ASCII"""

import sys
from _typeshed import ReadableBuffer
from typing import TypeAlias
from typing_extensions import deprecated

# Many functions in binascii accept buffer objects
# or ASCII-only strings.
_AsciiBuffer: TypeAlias = str | ReadableBuffer

def a2b_uu(data: _AsciiBuffer, /) -> bytes:
    """Decode a line of uuencoded data."""

def b2a_uu(data: ReadableBuffer, /, *, backtick: bool = False) -> bytes:
    """Uuencode line of data."""

if sys.version_info >= (3, 15):
    ASCII85_ALPHABET: bytes
    BINHEX_ALPHABET: bytes
    CRYPT_ALPHABET: bytes
    UU_ALPHABET: bytes
    BASE64_ALPHABET: bytes
    URLSAFE_BASE64_ALPHABET: bytes
    BASE32_ALPHABET: bytes
    BASE32HEX_ALPHABET: bytes
    BASE85_ALPHABET: bytes
    Z85_ALPHABET: bytes
    def a2b_base64(
        data: _AsciiBuffer,
        /,
        *,
        strict_mode: bool = False,
        alphabet: bytes = ...,
        padded: bool = True,
        ignorechars: ReadableBuffer = ...,
        canonical: bool = False,
    ) -> bytes:
        """Decode a line of base64 data.

        strict_mode
          When set to true, bytes that are not part of the base64 standard are
          not allowed.  The same applies to excess data after padding (= / ==).
          Set to True by default if ignorechars is specified, False otherwise.
        padded
          When set to false, padding in input is not required.
        ignorechars
          A byte string containing characters to ignore from the input when
          strict_mode is true.
        canonical
          When set to true, reject non-zero padding bits per RFC 4648 section 3.5.
        """

    def b2a_base64(
        data: ReadableBuffer, /, *, newline: bool = True, alphabet: ReadableBuffer = ..., padded: bool = True, wrapcol: int = 0
    ) -> bytes:
        """Base64-code line of data.

        padded
          When set to false, omit padding in the output.
        """

    def b2a_base32(data: ReadableBuffer, /, *, alphabet: ReadableBuffer = ..., padded: bool = True, wrapcol: int = 0) -> bytes:
        """Base32-code line of data.

        padded
          When set to false, omit padding in the output.
        """

    def a2b_base32(
        data: _AsciiBuffer,
        /,
        *,
        alphabet: bytes = ...,
        padded: bool = True,
        ignorechars: ReadableBuffer = b"",
        canonical: bool = False,
    ) -> bytes:
        """Decode a line of base32 data.

        padded
          When set to false, padding in input is not required.
        ignorechars
          A byte string containing characters to ignore from the input.
        canonical
          When set to true, reject non-zero padding bits per RFC 4648 section 3.5.
        """

    def b2a_ascii85(
        data: ReadableBuffer, /, *, foldspaces: bool = False, wrapcol: int = 0, pad: bool = False, adobe: bool = False
    ) -> bytes:
        """Ascii85-encode data.

        foldspaces
          Emit 'y' as a short form encoding four spaces.
        wrapcol
          Split result into lines of provided width.
        pad
          Retain zero-padding bytes at end of output.
        adobe
          Wrap result in '<~' and '~>' as in Adobe Ascii85.
        """

    def a2b_ascii85(
        data: _AsciiBuffer,
        /,
        *,
        foldspaces: bool = False,
        adobe: bool = False,
        ignorechars: ReadableBuffer = b"",
        canonical: bool = False,
    ) -> bytes:
        """Decode Ascii85 data.

        foldspaces
          Allow 'y' as a short form encoding four spaces.
        adobe
          Expect data to be terminated with '~>' as in Adobe Ascii85, and
          optionally accept leading '<~'.
        ignorechars
          A byte string containing characters to ignore from the input.
        canonical
          When set to true, reject non-canonical encodings.
        """

    def b2a_base85(data: ReadableBuffer, /, *, alphabet: ReadableBuffer = ..., pad: bool = False, wrapcol: int = 0) -> bytes:
        """Base85-code line of data.

        pad
          Retain zero-padding bytes at end of output.
        """

    def a2b_base85(
        data: _AsciiBuffer, /, *, alphabet: bytes = ..., ignorechars: ReadableBuffer = b"", canonical: bool = False
    ) -> bytes:
        """Decode a line of Base85 data.

        ignorechars
          A byte string containing characters to ignore from the input.
        canonical
          When set to true, reject non-canonical encodings.
        """

elif sys.version_info >= (3, 11):
    def a2b_base64(data: _AsciiBuffer, /, *, strict_mode: bool = False) -> bytes:
        """Decode a line of base64 data.

        strict_mode
          When set to True, bytes that are not part of the base64 standard are not allowed.
          The same applies to excess data after padding (= / ==).
        """

else:
    def a2b_base64(data: _AsciiBuffer, /) -> bytes:
        """Decode a line of base64 data."""

if sys.version_info < (3, 15):
    def b2a_base64(data: ReadableBuffer, /, *, newline: bool = True) -> bytes:
        """Base64-code line of data."""

def a2b_qp(data: _AsciiBuffer, header: bool = False) -> bytes:
    """Decode a string of qp-encoded data."""

def b2a_qp(data: ReadableBuffer, quotetabs: bool = False, istext: bool = True, header: bool = False) -> bytes:
    """Encode a string using quoted-printable encoding.

    On encoding, when istext is set, newlines are not encoded, and white
    space at end of lines is.  When istext is not set, \\r and \\n (CR/LF)
    are both encoded.  When quotetabs is set, space and tabs are encoded.
    """

if sys.version_info < (3, 11):
    @deprecated("Deprecated since Python 3.9; removed in Python 3.11.")
    def a2b_hqx(data: _AsciiBuffer, /) -> bytes:
        """Decode .hqx coding."""

    @deprecated("Deprecated since Python 3.9; removed in Python 3.11.")
    def rledecode_hqx(data: ReadableBuffer, /) -> bytes:
        """Decode hexbin RLE-coded string."""

    @deprecated("Deprecated since Python 3.9; removed in Python 3.11.")
    def rlecode_hqx(data: ReadableBuffer, /) -> bytes:
        """Binhex RLE-code binary data."""

    @deprecated("Deprecated since Python 3.9; removed in Python 3.11.")
    def b2a_hqx(data: ReadableBuffer, /) -> bytes:
        """Encode .hqx data."""

def crc_hqx(data: ReadableBuffer, crc: int, /) -> int:
    """Compute CRC-CCITT incrementally."""

def crc32(data: ReadableBuffer, crc: int = 0, /) -> int:
    """Compute CRC-32 incrementally."""

def b2a_hex(data: ReadableBuffer, sep: str | bytes = ..., bytes_per_sep: int = 1) -> bytes:
    """Hexadecimal representation of binary data.

      sep
        An optional single character or byte to separate hex bytes.
      bytes_per_sep
        How many bytes between separators.  Positive values count from the
        right, negative values count from the left.

    The return value is a bytes object.  This function is also
    available as "hexlify()".

    Example:
    >>> binascii.b2a_hex(b'\\xb9\\x01\\xef')
    b'b901ef'
    >>> binascii.hexlify(b'\\xb9\\x01\\xef', ':')
    b'b9:01:ef'
    >>> binascii.b2a_hex(b'\\xb9\\x01\\xef', b'_', 2)
    b'b9_01ef'
    """

def hexlify(data: ReadableBuffer, sep: str | bytes = ..., bytes_per_sep: int = 1) -> bytes:
    """Hexadecimal representation of binary data.

      sep
        An optional single character or byte to separate hex bytes.
      bytes_per_sep
        How many bytes between separators.  Positive values count from the
        right, negative values count from the left.

    The return value is a bytes object.  This function is also
    available as "b2a_hex()".
    """

if sys.version_info >= (3, 15):
    def a2b_hex(hexstr: _AsciiBuffer, /, *, ignorechars: ReadableBuffer = b"") -> bytes:
        """Binary data of hexadecimal representation.

          ignorechars
            A byte string containing characters to ignore from the input.

        hexstr must contain an even number of hex digits (upper or lower case).
        This function is also available as "unhexlify()".
        """

    def unhexlify(hexstr: _AsciiBuffer, /, *, ignorechars: ReadableBuffer = b"") -> bytes:
        """Binary data of hexadecimal representation.

          ignorechars
            A byte string containing characters to ignore from the input.

        hexstr must contain an even number of hex digits (upper or lower case).
        """

else:
    def a2b_hex(hexstr: _AsciiBuffer, /) -> bytes:
        """Binary data of hexadecimal representation.

        hexstr must contain an even number of hex digits (upper or lower case).
        This function is also available as "unhexlify()".
        """

    def unhexlify(hexstr: _AsciiBuffer, /) -> bytes:
        """Binary data of hexadecimal representation.

        hexstr must contain an even number of hex digits (upper or lower case).
        """

class Error(ValueError): ...
class Incomplete(Exception): ...

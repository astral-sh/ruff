"""Conversion between binary data and ASCII"""

import sys
from _typeshed import ReadableBuffer
from typing_extensions import TypeAlias, deprecated

# Many functions in binascii accept buffer objects
# or ASCII-only strings.
_AsciiBuffer: TypeAlias = str | ReadableBuffer

def a2b_uu(data: _AsciiBuffer, /) -> bytes:
    """Decode a line of uuencoded data."""

def b2a_uu(data: ReadableBuffer, /, *, backtick: bool = False) -> bytes:
    """Uuencode line of data."""

if sys.version_info >= (3, 11):
    def a2b_base64(data: _AsciiBuffer, /, *, strict_mode: bool = False) -> bytes:
        """Decode a line of base64 data.

        strict_mode
          When set to True, bytes that are not part of the base64 standard are not allowed.
          The same applies to excess data after padding (= / ==).
        """

else:
    def a2b_base64(data: _AsciiBuffer, /) -> bytes:
        """Decode a line of base64 data."""

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

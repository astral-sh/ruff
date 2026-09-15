"""Base64 content transfer encoding per RFCs 2045-2047.

This module handles the content transfer encoding method defined in RFC 2045
to encode arbitrary 8-bit data using the three 8-bit bytes in four 7-bit
characters encoding known as Base64.

It is used in the MIME standards for email to attach images, audio, and text
using some 8-bit character sets to messages.

This module provides an interface to encode and decode both headers and bodies
with Base64 encoding.

RFC 2045 defines a method for including character set information in an
'encoded-word' in a header.  This method is commonly used for 8-bit real names
in To:, From:, Cc:, etc. fields, as well as Subject: lines.

This module does not do the line wrapping or end-of-line character conversion
necessary for proper internationalized headers; it only does dumb encoding and
decoding.  To deal with the various line wrapping issues, use the email.header
module.
"""

__all__ = ["body_decode", "body_encode", "decode", "decodestring", "header_encode", "header_length"]

from _typeshed import ReadableBuffer

def header_length(bytearray: str | bytes | bytearray) -> int:
    """Return the length of s when it is encoded with base64."""

def header_encode(header_bytes: str | ReadableBuffer, charset: str = "iso-8859-1") -> str:
    """Encode a single header line with Base64 encoding in a given charset.

    charset names the character set to use to encode the header.  It defaults
    to iso-8859-1.  Base64 encoding is defined in RFC 2045.
    """

# First argument should be a buffer that supports slicing and len().
def body_encode(s: bytes | bytearray, maxlinelen: int = 76, eol: str = "\n") -> str:
    """Encode a string with base64.

    Each line will be wrapped at, at most, maxlinelen characters (defaults to
    76 characters).

    Each line of encoded text will end with eol, which defaults to "\\n".  Set
    this to "\\r\\n" if you will be using the result of this function directly
    in an email.
    """

def decode(string: str | ReadableBuffer) -> bytes:
    """Decode a raw base64 string, returning a bytes object.

    This function does not parse a full MIME header value encoded with
    base64 (like =?iso-8859-1?b?bmloISBuaWgh?=) -- please use the high
    level email.header class for that functionality.
    """

body_decode = decode
decodestring = decode

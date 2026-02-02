"""Quoted-printable content transfer encoding per RFCs 2045-2047.

This module handles the content transfer encoding method defined in RFC 2045
to encode US ASCII-like 8-bit data called 'quoted-printable'.  It is used to
safely encode text that is in a character set similar to the 7-bit US ASCII
character set, but that includes some 8-bit characters that are normally not
allowed in email bodies or headers.

Quoted-printable is very space-inefficient for encoding binary files; use the
email.base64mime module for that instead.

This module provides an interface to encode and decode both headers and bodies
with quoted-printable encoding.

RFC 2045 defines a method for including character set information in an
'encoded-word' in a header.  This method is commonly used for 8-bit real names
in To:/From:/Cc: etc. fields, as well as Subject: lines.

This module does not do the line wrapping or end-of-line character
conversion necessary for proper internationalized headers; it only
does dumb encoding and decoding.  To deal with the various line
wrapping issues, use the email.header module.
"""

from collections.abc import Iterable

__all__ = [
    "body_decode",
    "body_encode",
    "body_length",
    "decode",
    "decodestring",
    "header_decode",
    "header_encode",
    "header_length",
    "quote",
    "unquote",
]

def header_check(octet: int) -> bool:
    """Return True if the octet should be escaped with header quopri."""

def body_check(octet: int) -> bool:
    """Return True if the octet should be escaped with body quopri."""

def header_length(bytearray: Iterable[int]) -> int:
    """Return a header quoted-printable encoding length.

    Note that this does not include any RFC 2047 chrome added by
    `header_encode()`.

    :param bytearray: An array of bytes (a.k.a. octets).
    :return: The length in bytes of the byte array when it is encoded with
        quoted-printable for headers.
    """

def body_length(bytearray: Iterable[int]) -> int:
    """Return a body quoted-printable encoding length.

    :param bytearray: An array of bytes (a.k.a. octets).
    :return: The length in bytes of the byte array when it is encoded with
        quoted-printable for bodies.
    """

def unquote(s: str | bytes | bytearray) -> str:
    """Turn a string in the form =AB to the ASCII character with value 0xab"""

def quote(c: str | bytes | bytearray) -> str: ...
def header_encode(header_bytes: bytes | bytearray, charset: str = "iso-8859-1") -> str:
    """Encode a single header line with quoted-printable (like) encoding.

    Defined in RFC 2045, this 'Q' encoding is similar to quoted-printable, but
    used specifically for email header fields to allow charsets with mostly 7
    bit characters (and some 8 bit) to remain more or less readable in non-RFC
    2045 aware mail clients.

    charset names the character set to use in the RFC 2046 header.  It
    defaults to iso-8859-1.
    """

def body_encode(body: str, maxlinelen: int = 76, eol: str = "\n") -> str:
    """Encode with quoted-printable, wrapping at maxlinelen characters.

    Each line of encoded text will end with eol, which defaults to "\\n".  Set
    this to "\\r\\n" if you will be using the result of this function directly
    in an email.

    Each line will be wrapped at, at most, maxlinelen characters before the
    eol string (maxlinelen defaults to 76 characters, the maximum value
    permitted by RFC 2045).  Long lines will have the 'soft line break'
    quoted-printable character "=" appended to them, so the decoded text will
    be identical to the original text.

    The minimum maxlinelen is 4 to have room for a quoted character ("=XX")
    followed by a soft line break.  Smaller values will generate a
    ValueError.

    """

def decode(encoded: str, eol: str = "\n") -> str:
    """Decode a quoted-printable string.

    Lines are separated with eol, which defaults to \\n.
    """

def header_decode(s: str) -> str:
    """Decode a string encoded with RFC 2045 MIME header 'Q' encoding.

    This function does not parse a full MIME header value encoded with
    quoted-printable (like =?iso-8859-1?q?Hello_World?=) -- please use
    the high level email.header class for that functionality.
    """

body_decode = decode
decodestring = decode

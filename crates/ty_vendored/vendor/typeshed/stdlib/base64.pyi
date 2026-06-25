"""Base16, Base32, Base64 (RFC 4648), Base85 and Ascii85 data encodings"""

import sys
from _typeshed import ReadableBuffer, SupportsNoArgReadline, SupportsRead, SupportsWrite

__all__ = [
    "encode",
    "decode",
    "encodebytes",
    "decodebytes",
    "b64encode",
    "b64decode",
    "b32encode",
    "b32decode",
    "b16encode",
    "b16decode",
    "b32hexencode",
    "b32hexdecode",
    "b85encode",
    "b85decode",
    "a85encode",
    "a85decode",
    "standard_b64encode",
    "standard_b64decode",
    "urlsafe_b64encode",
    "urlsafe_b64decode",
]

if sys.version_info >= (3, 13):
    __all__ += ["z85decode", "z85encode"]

if sys.version_info >= (3, 15):
    def b64encode(s: ReadableBuffer, altchars: ReadableBuffer | None = None, *, padded: bool = True, wrapcol: int = 0) -> bytes:
        """Encode the bytes-like object s using Base64 and return a bytes object.

        Optional altchars should be a byte string of length 2 which specifies an
        alternative alphabet for the '+' and '/' characters.  This allows an
        application to e.g. generate url or filesystem safe Base64 strings.

        If padded is false, omit padding in the output.

        If wrapcol is non-zero, insert a newline (b'\\n') character after at most
        every wrapcol characters.
        """

    def b64decode(
        s: str | ReadableBuffer,
        altchars: str | ReadableBuffer | None = None,
        validate: bool = ...,
        *,
        padded: bool = True,
        ignorechars: ReadableBuffer = ...,
        canonical: bool = False,
    ) -> bytes:
        """Decode the Base64 encoded bytes-like object or ASCII string s.

        Optional altchars must be a bytes-like object or ASCII string of length 2
        which specifies the alternative alphabet used instead of the '+' and '/'
        characters.

        If padded is false, padding in input is not required.

        The result is returned as a bytes object.  A binascii.Error is raised if
        s is incorrectly padded.

        If ignorechars is specified, it should be a byte string containing
        characters to ignore from the input.  The default value of validate is
        True if ignorechars is specified, False otherwise.

        If validate is false, characters that are neither in the normal base-64
        alphabet nor the alternative alphabet are discarded prior to the
        padding check.  If validate is true, these non-alphabet characters in
        the input result in a binascii.Error if they are not in ignorechars.
        For more information about the strict base64 check, see:

        https://docs.python.org/3.11/library/binascii.html#binascii.a2b_base64
        """

else:
    def b64encode(s: ReadableBuffer, altchars: ReadableBuffer | None = None) -> bytes:
        """Encode the bytes-like object s using Base64 and return a bytes object.

        Optional altchars should be a byte string of length 2 which specifies an
        alternative alphabet for the '+' and '/' characters.  This allows an
        application to e.g. generate url or filesystem safe Base64 strings.
        """

    def b64decode(s: str | ReadableBuffer, altchars: str | ReadableBuffer | None = None, validate: bool = False) -> bytes:
        """Decode the Base64 encoded bytes-like object or ASCII string s.

        Optional altchars must be a bytes-like object or ASCII string of length 2
        which specifies the alternative alphabet used instead of the '+' and '/'
        characters.

        The result is returned as a bytes object.  A binascii.Error is raised if
        s is incorrectly padded.

        If validate is False (the default), characters that are neither in the
        normal base-64 alphabet nor the alternative alphabet are discarded prior
        to the padding check.  If validate is True, these non-alphabet characters
        in the input result in a binascii.Error.
        For more information about the strict base64 check, see:

        https://docs.python.org/3.11/library/binascii.html#binascii.a2b_base64
        """

def standard_b64encode(s: ReadableBuffer) -> bytes:
    """Encode bytes-like object s using the standard Base64 alphabet.

    The result is returned as a bytes object.
    """

def standard_b64decode(s: str | ReadableBuffer) -> bytes:
    """Decode bytes encoded with the standard Base64 alphabet.

    Argument s is a bytes-like object or ASCII string to decode.  The result
    is returned as a bytes object.  A binascii.Error is raised if the input
    is incorrectly padded.  Characters that are not in the standard alphabet
    are discarded prior to the padding check.
    """

if sys.version_info >= (3, 15):
    def urlsafe_b64encode(s: ReadableBuffer, *, padded: bool = True) -> bytes:
        """Encode bytes using the URL- and filesystem-safe Base64 alphabet.

        Argument s is a bytes-like object to encode.  The result is returned as a
        bytes object.  The alphabet uses '-' instead of '+' and '_' instead of
        '/'.

        If padded is false, omit padding in the output.
        """

    def urlsafe_b64decode(s: str | ReadableBuffer, *, padded: bool = False) -> bytes:
        """Decode bytes using the URL- and filesystem-safe Base64 alphabet.

        Argument s is a bytes-like object or ASCII string to decode.  The result
        is returned as a bytes object.  A binascii.Error is raised if the input
        is incorrectly padded.  Characters that are not in the URL-safe base-64
        alphabet, and are not a plus '+' or slash '/', are discarded prior to the
        padding check.

        If padded is false, padding in input is not required.

        The alphabet uses '-' instead of '+' and '_' instead of '/'.
        """

    def b32encode(s: ReadableBuffer, *, padded: bool = True, wrapcol: int = 0) -> bytes:
        """
        Encode the bytes-like objects using base32 and return a bytes object.

        If padded is false, omit padding in the output.

        If wrapcol is non-zero, insert a newline (b'\\n') character after at most
        every wrapcol characters.
        """

    def b32decode(
        s: str | ReadableBuffer,
        casefold: bool = False,
        map01: str | ReadableBuffer | None = None,
        *,
        padded: bool = True,
        ignorechars: ReadableBuffer = b"",
        canonical: bool = False,
    ) -> bytes:
        """
        Decode the base32 encoded bytes-like object or ASCII string s.

        Optional casefold is a flag specifying whether a lowercase alphabet is
        acceptable as input.  For security purposes, the default is False.

        If padded is false, padding in input is not required.

        ignorechars should be a byte string containing characters to ignore
        from the input.

        RFC 4648 allows for optional mapping of the digit 0 (zero) to the
        letter O (oh), and for optional mapping of the digit 1 (one) to
        either the letter I (eye) or letter L (el).  The optional argument
        map01 when not None, specifies which letter the digit 1 should be
        mapped to (when map01 is not None, the digit 0 is always mapped to
        the letter O).  For security purposes the default is None, so that
        0 and 1 are not allowed in the input.

        The result is returned as a bytes object.  A binascii.Error is raised if
        the input is incorrectly padded or if there are non-alphabet
        characters present in the input.
        """

    def b16encode(s: ReadableBuffer, *, wrapcol: int = 0) -> bytes:
        """Encode the bytes-like object s using Base16 and return a bytes object.

        If wrapcol is non-zero, insert a newline (b'\\n') character after at most
        every wrapcol characters.
        """

    def b16decode(s: str | ReadableBuffer, casefold: bool = False, *, ignorechars: ReadableBuffer = b"") -> bytes:
        """Decode the Base16 encoded bytes-like object or ASCII string s.

        Optional casefold is a flag specifying whether a lowercase alphabet is
        acceptable as input.  For security purposes, the default is False.

        ignorechars should be a byte string containing characters to ignore
        from the input.

        The result is returned as a bytes object.  A binascii.Error is raised if
        s is incorrectly padded or if there are non-alphabet characters present
        in the input.
        """

else:
    def urlsafe_b64encode(s: ReadableBuffer) -> bytes:
        """Encode bytes using the URL- and filesystem-safe Base64 alphabet.

        Argument s is a bytes-like object to encode.  The result is returned as a
        bytes object.  The alphabet uses '-' instead of '+' and '_' instead of
        '/'.
        """

    def urlsafe_b64decode(s: str | ReadableBuffer) -> bytes:
        """Decode bytes using the URL- and filesystem-safe Base64 alphabet.

        Argument s is a bytes-like object or ASCII string to decode.  The result
        is returned as a bytes object.  A binascii.Error is raised if the input
        is incorrectly padded.  Characters that are not in the URL-safe base-64
        alphabet, and are not a plus '+' or slash '/', are discarded prior to the
        padding check.

        The alphabet uses '-' instead of '+' and '_' instead of '/'.
        """

    def b32encode(s: ReadableBuffer) -> bytes:
        """
        Encode the bytes-like objects using base32 and return a bytes object.
        """

    def b32decode(s: str | ReadableBuffer, casefold: bool = False, map01: str | ReadableBuffer | None = None) -> bytes:
        """
        Decode the base32 encoded bytes-like object or ASCII string s.

        Optional casefold is a flag specifying whether a lowercase alphabet is
        acceptable as input.  For security purposes, the default is False.

        RFC 3548 allows for optional mapping of the digit 0 (zero) to the
        letter O (oh), and for optional mapping of the digit 1 (one) to
        either the letter I (eye) or letter L (el).  The optional argument
        map01 when not None, specifies which letter the digit 1 should be
        mapped to (when map01 is not None, the digit 0 is always mapped to
        the letter O).  For security purposes the default is None, so that
        0 and 1 are not allowed in the input.

        The result is returned as a bytes object.  A binascii.Error is raised if
        the input is incorrectly padded or if there are non-alphabet
        characters present in the input.
        """

    def b16encode(s: ReadableBuffer) -> bytes:
        """Encode the bytes-like object s using Base16 and return a bytes object."""

    def b16decode(s: str | ReadableBuffer, casefold: bool = False) -> bytes:
        """Decode the Base16 encoded bytes-like object or ASCII string s.

        Optional casefold is a flag specifying whether a lowercase alphabet is
        acceptable as input.  For security purposes, the default is False.

        The result is returned as a bytes object.  A binascii.Error is raised if
        s is incorrectly padded or if there are non-alphabet characters present
        in the input.
        """

if sys.version_info >= (3, 15):
    def b32hexencode(s: ReadableBuffer, *, padded: bool = True, wrapcol: int = 0) -> bytes:
        """
        Encode the bytes-like objects using base32hex and return a bytes object.

        If padded is false, omit padding in the output.

        If wrapcol is non-zero, insert a newline (b'\\n') character after at most
        every wrapcol characters.
        """

    def b32hexdecode(
        s: str | ReadableBuffer,
        casefold: bool = False,
        *,
        padded: bool = True,
        ignorechars: ReadableBuffer = b"",
        canonical: bool = False,
    ) -> bytes:
        """
        Decode the base32hex encoded bytes-like object or ASCII string s.

        Optional casefold is a flag specifying whether a lowercase alphabet is
        acceptable as input.  For security purposes, the default is False.

        If padded is false, padding in input is not required.

        ignorechars should be a byte string containing characters to ignore
        from the input.

        The result is returned as a bytes object.  A binascii.Error is raised if
        the input is incorrectly padded or if there are non-alphabet
        characters present in the input.
        """

else:
    def b32hexencode(s: ReadableBuffer) -> bytes:
        """
        Encode the bytes-like objects using base32hex and return a bytes object.
        """

    def b32hexdecode(s: str | ReadableBuffer, casefold: bool = False) -> bytes:
        """
        Decode the base32hex encoded bytes-like object or ASCII string s.

        Optional casefold is a flag specifying whether a lowercase alphabet is
        acceptable as input.  For security purposes, the default is False.

        The result is returned as a bytes object.  A binascii.Error is raised if
        the input is incorrectly padded or if there are non-alphabet
        characters present in the input.
        """

def a85encode(b: ReadableBuffer, *, foldspaces: bool = False, wrapcol: int = 0, pad: bool = False, adobe: bool = False) -> bytes:
    """Encode bytes-like object b using Ascii85 and return a bytes object.

    foldspaces is an optional flag that uses the special short sequence 'y'
    instead of 4 consecutive spaces (ASCII 0x20) as supported by 'btoa'. This
    feature is not supported by the standard encoding used in PDF.

    If wrapcol is non-zero, insert a newline (b'\\n') character after at most
    every wrapcol characters.

    pad controls whether zero-padding applied to the end of the input
    is fully retained in the output encoding, as done by btoa,
    producing an exact multiple of 5 bytes of output.

    adobe controls whether the encoded byte sequence is framed with <~
    and ~>, as in a PostScript base-85 string literal.  Note that
    while ASCII85Decode streams in PDF documents must be terminated
    with ~>, they must not use a leading <~.

    """

if sys.version_info >= (3, 15):
    def a85decode(
        b: str | ReadableBuffer,
        *,
        foldspaces: bool = False,
        adobe: bool = False,
        ignorechars: bytearray | bytes = b" \t\n\r\x0b",
        canonical: bool = False,
    ) -> bytes:
        """Decode the Ascii85 encoded bytes-like object or ASCII string b.

        foldspaces is a flag that specifies whether the 'y' short sequence
        should be accepted as shorthand for 4 consecutive spaces (ASCII
        0x20).  This feature is not supported by the standard Ascii85
        encoding used in PDF and PostScript.

        adobe controls whether the <~ and ~> markers are present. While
        the leading <~ is not required, the input must end with ~>, or a
        ValueError is raised.

        ignorechars should be a byte string containing characters to ignore from the
        input. This should only contain whitespace characters, and by default
        contains all whitespace characters in ASCII.

        If canonical is true, non-canonical encodings are rejected.

        The result is returned as a bytes object.
        """

    def b85encode(b: ReadableBuffer, pad: bool = False, *, wrapcol: int = 0) -> bytes:
        """Encode bytes-like object b in base85 format and return a bytes object.

        If wrapcol is non-zero, insert a newline (b'\\n') character after at most
        every wrapcol characters.

        The input is padded with b'\x00' so its length is a multiple of 4
        bytes before encoding.  If pad is true, all the resulting
        characters are retained in the output, which will always be a
        multiple of 5 bytes.
        """

    def b85decode(b: str | ReadableBuffer, *, ignorechars: ReadableBuffer = b"", canonical: bool = False) -> bytes:
        """Decode the base85-encoded bytes-like object or ASCII string b

        If canonical is true, non-canonical encodings are rejected.

        The result is returned as a bytes object.
        """

else:
    def a85decode(
        b: str | ReadableBuffer, *, foldspaces: bool = False, adobe: bool = False, ignorechars: bytearray | bytes = b" \t\n\r\x0b"
    ) -> bytes:
        """Decode the Ascii85 encoded bytes-like object or ASCII string b.

        foldspaces is a flag that specifies whether the 'y' short sequence
        should be accepted as shorthand for 4 consecutive spaces (ASCII
        0x20).  This feature is not supported by the standard Ascii85
        encoding used in PDF and PostScript.

        adobe controls whether the <~ and ~> markers are present. While
        the leading <~ is not required, the input must end with ~>, or a
        ValueError is raised.

        ignorechars should be a byte string containing characters to ignore from the
        input. This should only contain whitespace characters, and by default
        contains all whitespace characters in ASCII.

        The result is returned as a bytes object.
        """

    def b85encode(b: ReadableBuffer, pad: bool = False) -> bytes:
        """Encode bytes-like object b in base85 format and return a bytes object.

        The input is padded with b'\x00' so its length is a multiple of 4
        bytes before encoding.  If pad is true, all the resulting
        characters are retained in the output, which will always be a
        multiple of 5 bytes.
        """

    def b85decode(b: str | ReadableBuffer) -> bytes:
        """Decode the base85-encoded bytes-like object or ASCII string b

        The result is returned as a bytes object.
        """

def decode(input: SupportsNoArgReadline[bytes], output: SupportsWrite[bytes]) -> None:
    """Decode a file; input and output are binary files."""

def encode(input: SupportsRead[bytes], output: SupportsWrite[bytes]) -> None:
    """Encode a file; input and output are binary files."""

def encodebytes(s: ReadableBuffer) -> bytes:
    """Encode a bytestring into a bytes object containing multiple lines
    of base-64 data.
    """

def decodebytes(s: ReadableBuffer) -> bytes:
    """Decode a bytestring of base-64 data into a bytes object."""

if sys.version_info >= (3, 13):
    if sys.version_info >= (3, 15):
        def z85encode(s: ReadableBuffer, pad: bool = False, *, wrapcol: int = 0) -> bytes:
            """Encode bytes-like object b in z85 format and return a bytes object.

            If wrapcol is non-zero, insert a newline (b'\\n') character after at most
            every wrapcol characters.

            The input is padded with b'\x00' so its length is a multiple of
            bytes before encoding.  If pad is true, all the resulting
            characters are retained in the output, which will always be a
            multiple of 5 bytes, as required by the ZeroMQ standard.
            """

        def z85decode(s: str | ReadableBuffer, *, ignorechars: ReadableBuffer = b"", canonical: bool = False) -> bytes:
            """Decode the z85-encoded bytes-like object or ASCII string b

            If canonical is true, non-canonical encodings are rejected.

            The result is returned as a bytes object.
            """

    else:
        def z85encode(s: ReadableBuffer) -> bytes:
            """Encode bytes-like object b in z85 format and return a bytes object."""

        def z85decode(s: str | ReadableBuffer) -> bytes:
            """Decode the z85-encoded bytes-like object or ASCII string b

            The result is returned as a bytes object.
            """

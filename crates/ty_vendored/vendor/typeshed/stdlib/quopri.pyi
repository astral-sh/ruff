"""Conversions to/from quoted-printable transport encoding as per RFC 1521."""

from _typeshed import ReadableBuffer, SupportsNoArgReadline, SupportsRead, SupportsWrite
from typing import Protocol, type_check_only

__all__ = ["encode", "decode", "encodestring", "decodestring"]

@type_check_only
class _Input(SupportsRead[bytes], SupportsNoArgReadline[bytes], Protocol): ...

def encode(input: _Input, output: SupportsWrite[bytes], quotetabs: int, header: bool = False) -> None:
    """Read 'input', apply quoted-printable encoding, and write to 'output'.

    'input' and 'output' are binary file objects. The 'quotetabs' flag
    indicates whether embedded tabs and spaces should be quoted. Note that
    line-ending tabs and spaces are always encoded, as per RFC 1521.
    The 'header' flag indicates whether we are encoding spaces as _ as per RFC
    1522.
    """

def encodestring(s: ReadableBuffer, quotetabs: bool = False, header: bool = False) -> bytes: ...
def decode(input: _Input, output: SupportsWrite[bytes], header: bool = False) -> None:
    """Read 'input', apply quoted-printable decoding, and write to 'output'.
    'input' and 'output' are binary file objects.
    If 'header' is true, decode underscore as space (per RFC 1522).
    """

def decodestring(s: str | ReadableBuffer, header: bool = False) -> bytes: ...

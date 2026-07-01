"""Functions to convert between Python values and C structs.
Python bytes objects are used to hold the data representing the C struct.
The format string (explained below) describes the layout of data
in the C struct.

The optional first format char indicates byte order, size and alignment:
  @: native order, size & alignment (default)
  =: native order, std. size & alignment
  <: little-endian, std. size & alignment
  >: big-endian, std. size & alignment
  !: same as >

The remaining characters indicate types of args and must match exactly;
these can be preceded by a decimal repeat count:
  x: pad byte (no data); c: char; b: signed byte; B: unsigned byte;
  ?: _Bool; h: short; H: unsigned short; i: int; I: unsigned int;
  l: long; L: unsigned long; q: long long; Q: unsigned long long;
  f: float; d: double; e: half-float;
  F: float complex; D: double complex.
Special cases (preceding decimal count indicates length):
  s: byte string (array of char); p: Pascal string (with count byte).
Special cases (only available in native format):
  n: ssize_t; N: size_t;
  P: an integer type that is wide enough to hold a pointer.
Whitespace between formats is ignored.

The variable struct.error is an exception raised on errors.
"""

from _typeshed import ReadableBuffer, WriteableBuffer
from collections.abc import Iterator
from typing import Any
from typing_extensions import disjoint_base

def pack(fmt: str | bytes, /, *v: Any) -> bytes:
    """Pack values and return the packed bytes.

    Return a bytes object containing the provided values packed according
    to the format string.  See help(struct) for more on format strings.
    """

def pack_into(fmt: str | bytes, buffer: WriteableBuffer, offset: int, /, *v: Any) -> None:
    """Pack values and write the packed bytes into the buffer.

    Pack the provided values according to the format string and write the
    packed bytes into the writable buffer starting at offset.  Note that the
    offset is a required argument.  See help(struct) for more on format
    strings.
    """

def unpack(format: str | bytes, buffer: ReadableBuffer, /) -> tuple[Any, ...]:
    """Return a tuple containing values unpacked according to the format string.

    The buffer's size in bytes must be calcsize(format).  See help(struct)
    for more on format strings.
    """

def unpack_from(format: str | bytes, /, buffer: ReadableBuffer, offset: int = 0) -> tuple[Any, ...]:
    """Return a tuple containing values unpacked according to the format string.

    The buffer's size, minus offset, must be at least calcsize(format).  See
    help(struct) for more on format strings.
    """

def iter_unpack(format: str | bytes, buffer: ReadableBuffer, /) -> Iterator[tuple[Any, ...]]:
    """Return an iterator yielding tuples unpacked from the given bytes.

    The bytes are unpacked according to the format string, like a repeated
    invocation of unpack_from().  Requires that the bytes length be
    a multiple of calcsize(format).
    """

def calcsize(format: str | bytes, /) -> int:
    """Return size in bytes of the struct described by the format string."""

@disjoint_base
class Struct:
    """Create a compiled struct object.

    Return a new Struct object which writes and reads binary data according
    to the format string.  See help(struct) for more on format strings.
    """

    @property
    def format(self) -> str:
        """struct format string"""

    @property
    def size(self) -> int:
        """struct size in bytes"""

    def __init__(self, format: str | bytes) -> None: ...
    def pack(self, *v: Any) -> bytes:
        """Pack values and return the packed bytes.

        Return a bytes object containing the provided values packed
        according to the struct format string.  See help(struct) for more on
        format strings.
        """

    def pack_into(self, buffer: WriteableBuffer, offset: int, *v: Any) -> None:
        """Pack values and write the packed bytes into the buffer.

        Pack the provided values according to the struct format string
        and write the packed bytes into the writable buffer starting at
        offset.  Note that the offset is a required argument.  See
        help(struct) for more on format strings.
        """

    def unpack(self, buffer: ReadableBuffer, /) -> tuple[Any, ...]:
        """Return a tuple containing unpacked values.

        Unpack according to the struct format string.  The buffer's
        size in bytes must be the struct size.  See help(struct) for more on
        format strings.
        """

    def unpack_from(self, buffer: ReadableBuffer, offset: int = 0) -> tuple[Any, ...]:
        """Return a tuple containing unpacked values.

        Values are unpacked according to the struct format string.  The
        buffer's size in bytes, starting at position offset, must be at
        least the struct size.  See help(struct) for more on format
        strings.
        """

    def iter_unpack(self, buffer: ReadableBuffer, /) -> Iterator[tuple[Any, ...]]:
        """Return an iterator yielding tuples.

        Tuples are unpacked from the given bytes source, like a repeated
        invocation of unpack_from().  Requires that the bytes length be
        a multiple of the struct size.
        """

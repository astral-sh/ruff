"""Interface to the libbzip2 compression library.

This module provides a file interface, classes for incremental
(de)compression, and functions for one-shot (de)compression.
"""

import sys
from _bz2 import BZ2Compressor as BZ2Compressor, BZ2Decompressor as BZ2Decompressor
from _typeshed import ReadableBuffer, StrOrBytesPath, WriteableBuffer
from collections.abc import Iterable
from io import TextIOWrapper
from typing import IO, Literal, Protocol, SupportsIndex, overload, type_check_only
from typing_extensions import Self, TypeAlias

if sys.version_info >= (3, 14):
    from compression._common._streams import BaseStream, _Reader
else:
    from _compression import BaseStream, _Reader

__all__ = ["BZ2File", "BZ2Compressor", "BZ2Decompressor", "open", "compress", "decompress"]

# The following attributes and methods are optional:
# def fileno(self) -> int: ...
# def close(self) -> object: ...
@type_check_only
class _ReadableFileobj(_Reader, Protocol): ...

@type_check_only
class _WritableFileobj(Protocol):
    def write(self, b: bytes, /) -> object: ...
    # The following attributes and methods are optional:
    # def fileno(self) -> int: ...
    # def close(self) -> object: ...

def compress(data: ReadableBuffer, compresslevel: int = 9) -> bytes:
    """Compress a block of data.

    compresslevel, if given, must be a number between 1 and 9.

    For incremental compression, use a BZ2Compressor object instead.
    """

def decompress(data: ReadableBuffer) -> bytes:
    """Decompress a block of data.

    For incremental decompression, use a BZ2Decompressor object instead.
    """

_ReadBinaryMode: TypeAlias = Literal["", "r", "rb"]
_WriteBinaryMode: TypeAlias = Literal["w", "wb", "x", "xb", "a", "ab"]
_ReadTextMode: TypeAlias = Literal["rt"]
_WriteTextMode: TypeAlias = Literal["wt", "xt", "at"]

@overload
def open(
    filename: _ReadableFileobj,
    mode: _ReadBinaryMode = "rb",
    compresslevel: int = 9,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> BZ2File:
    """Open a bzip2-compressed file in binary or text mode.

    The filename argument can be an actual filename (a str, bytes, or
    PathLike object), or an existing file object to read from or write
    to.

    The mode argument can be "r", "rb", "w", "wb", "x", "xb", "a" or
    "ab" for binary mode, or "rt", "wt", "xt" or "at" for text mode.
    The default mode is "rb", and the default compresslevel is 9.

    For binary mode, this function is equivalent to the BZ2File
    constructor: BZ2File(filename, mode, compresslevel). In this case,
    the encoding, errors and newline arguments must not be provided.

    For text mode, a BZ2File object is created, and wrapped in an
    io.TextIOWrapper instance with the specified encoding, error
    handling behavior, and line ending(s).

    """

@overload
def open(
    filename: _ReadableFileobj,
    mode: _ReadTextMode,
    compresslevel: int = 9,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: _WritableFileobj,
    mode: _WriteBinaryMode,
    compresslevel: int = 9,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> BZ2File: ...
@overload
def open(
    filename: _WritableFileobj,
    mode: _WriteTextMode,
    compresslevel: int = 9,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: StrOrBytesPath,
    mode: _ReadBinaryMode | _WriteBinaryMode = "rb",
    compresslevel: int = 9,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> BZ2File: ...
@overload
def open(
    filename: StrOrBytesPath,
    mode: _ReadTextMode | _WriteTextMode,
    compresslevel: int = 9,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: StrOrBytesPath | _ReadableFileobj | _WritableFileobj,
    mode: str,
    compresslevel: int = 9,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> BZ2File | TextIOWrapper: ...

class BZ2File(BaseStream, IO[bytes]):
    """A file object providing transparent bzip2 (de)compression.

    A BZ2File can act as a wrapper for an existing file object, or refer
    directly to a named file on disk.

    Note that BZ2File provides a *binary* file interface - data read is
    returned as bytes, and data to be written should be given as bytes.
    """

    def __enter__(self) -> Self: ...
    @overload
    def __init__(self, filename: _WritableFileobj, mode: _WriteBinaryMode, *, compresslevel: int = 9) -> None:
        """Open a bzip2-compressed file.

        If filename is a str, bytes, or PathLike object, it gives the
        name of the file to be opened. Otherwise, it should be a file
        object, which will be used to read or write the compressed data.

        mode can be 'r' for reading (default), 'w' for (over)writing,
        'x' for creating exclusively, or 'a' for appending. These can
        equivalently be given as 'rb', 'wb', 'xb', and 'ab'.

        If mode is 'w', 'x' or 'a', compresslevel can be a number between 1
        and 9 specifying the level of compression: 1 produces the least
        compression, and 9 (default) produces the most compression.

        If mode is 'r', the input file may be the concatenation of
        multiple compressed streams.
        """

    @overload
    def __init__(self, filename: _ReadableFileobj, mode: _ReadBinaryMode = "r", *, compresslevel: int = 9) -> None: ...
    @overload
    def __init__(
        self, filename: StrOrBytesPath, mode: _ReadBinaryMode | _WriteBinaryMode = "r", *, compresslevel: int = 9
    ) -> None: ...
    def read(self, size: int | None = -1) -> bytes:
        """Read up to size uncompressed bytes from the file.

        If size is negative or omitted, read until EOF is reached.
        Returns b'' if the file is already at EOF.
        """

    def read1(self, size: int = -1) -> bytes:
        """Read up to size uncompressed bytes, while trying to avoid
        making multiple reads from the underlying stream. Reads up to a
        buffer's worth of data if size is negative.

        Returns b'' if the file is at EOF.
        """

    def readline(self, size: SupportsIndex = -1) -> bytes:  # type: ignore[override]
        """Read a line of uncompressed bytes from the file.

        The terminating newline (if present) is retained. If size is
        non-negative, no more than size bytes will be read (in which
        case the line may be incomplete). Returns b'' if already at EOF.
        """

    def readinto(self, b: WriteableBuffer) -> int:
        """Read bytes into b.

        Returns the number of bytes read (0 for EOF).
        """

    def readlines(self, size: SupportsIndex = -1) -> list[bytes]:
        """Read a list of lines of uncompressed bytes from the file.

        size can be specified to control the number of lines read: no
        further lines will be read once the total size of the lines read
        so far equals or exceeds size.
        """

    def peek(self, n: int = 0) -> bytes:
        """Return buffered data without advancing the file position.

        Always returns at least one byte of data, unless at EOF.
        The exact number of bytes returned is unspecified.
        """

    def seek(self, offset: int, whence: int = 0) -> int:
        """Change the file position.

        The new position is specified by offset, relative to the
        position indicated by whence. Values for whence are:

            0: start of stream (default); offset must not be negative
            1: current stream position
            2: end of stream; offset must not be positive

        Returns the new file position.

        Note that seeking is emulated, so depending on the parameters,
        this operation may be extremely slow.
        """

    def write(self, data: ReadableBuffer) -> int:
        """Write a byte string to the file.

        Returns the number of uncompressed bytes written, which is
        always the length of data in bytes. Note that due to buffering,
        the file on disk may not reflect the data written until close()
        is called.
        """

    def writelines(self, seq: Iterable[ReadableBuffer]) -> None:
        """Write a sequence of byte strings to the file.

        Returns the number of uncompressed bytes written.
        seq can be any iterable yielding byte strings.

        Line separators are not added between the written byte strings.
        """

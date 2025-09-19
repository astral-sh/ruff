"""Interface to the liblzma compression library.

This module provides a class for reading and writing compressed files,
classes for incremental (de)compression, and convenience functions for
one-shot (de)compression.

These classes and functions support both the XZ and legacy LZMA
container formats, as well as raw compressed data streams.
"""

import sys
from _lzma import (
    CHECK_CRC32 as CHECK_CRC32,
    CHECK_CRC64 as CHECK_CRC64,
    CHECK_ID_MAX as CHECK_ID_MAX,
    CHECK_NONE as CHECK_NONE,
    CHECK_SHA256 as CHECK_SHA256,
    CHECK_UNKNOWN as CHECK_UNKNOWN,
    FILTER_ARM as FILTER_ARM,
    FILTER_ARMTHUMB as FILTER_ARMTHUMB,
    FILTER_DELTA as FILTER_DELTA,
    FILTER_IA64 as FILTER_IA64,
    FILTER_LZMA1 as FILTER_LZMA1,
    FILTER_LZMA2 as FILTER_LZMA2,
    FILTER_POWERPC as FILTER_POWERPC,
    FILTER_SPARC as FILTER_SPARC,
    FILTER_X86 as FILTER_X86,
    FORMAT_ALONE as FORMAT_ALONE,
    FORMAT_AUTO as FORMAT_AUTO,
    FORMAT_RAW as FORMAT_RAW,
    FORMAT_XZ as FORMAT_XZ,
    MF_BT2 as MF_BT2,
    MF_BT3 as MF_BT3,
    MF_BT4 as MF_BT4,
    MF_HC3 as MF_HC3,
    MF_HC4 as MF_HC4,
    MODE_FAST as MODE_FAST,
    MODE_NORMAL as MODE_NORMAL,
    PRESET_DEFAULT as PRESET_DEFAULT,
    PRESET_EXTREME as PRESET_EXTREME,
    LZMACompressor as LZMACompressor,
    LZMADecompressor as LZMADecompressor,
    LZMAError as LZMAError,
    _FilterChain,
    is_check_supported as is_check_supported,
)
from _typeshed import ReadableBuffer, StrOrBytesPath
from io import TextIOWrapper
from typing import IO, Literal, overload
from typing_extensions import Self, TypeAlias

if sys.version_info >= (3, 14):
    from compression._common._streams import BaseStream
else:
    from _compression import BaseStream

__all__ = [
    "CHECK_NONE",
    "CHECK_CRC32",
    "CHECK_CRC64",
    "CHECK_SHA256",
    "CHECK_ID_MAX",
    "CHECK_UNKNOWN",
    "FILTER_LZMA1",
    "FILTER_LZMA2",
    "FILTER_DELTA",
    "FILTER_X86",
    "FILTER_IA64",
    "FILTER_ARM",
    "FILTER_ARMTHUMB",
    "FILTER_POWERPC",
    "FILTER_SPARC",
    "FORMAT_AUTO",
    "FORMAT_XZ",
    "FORMAT_ALONE",
    "FORMAT_RAW",
    "MF_HC3",
    "MF_HC4",
    "MF_BT2",
    "MF_BT3",
    "MF_BT4",
    "MODE_FAST",
    "MODE_NORMAL",
    "PRESET_DEFAULT",
    "PRESET_EXTREME",
    "LZMACompressor",
    "LZMADecompressor",
    "LZMAFile",
    "LZMAError",
    "open",
    "compress",
    "decompress",
    "is_check_supported",
]

_OpenBinaryWritingMode: TypeAlias = Literal["w", "wb", "x", "xb", "a", "ab"]
_OpenTextWritingMode: TypeAlias = Literal["wt", "xt", "at"]

_PathOrFile: TypeAlias = StrOrBytesPath | IO[bytes]

class LZMAFile(BaseStream, IO[bytes]):  # type: ignore[misc]  # incompatible definitions of writelines in the base classes
    """A file object providing transparent LZMA (de)compression.

    An LZMAFile can act as a wrapper for an existing file object, or
    refer directly to a named file on disk.

    Note that LZMAFile provides a *binary* file interface - data read
    is returned as bytes, and data to be written must be given as bytes.
    """

    def __init__(
        self,
        filename: _PathOrFile | None = None,
        mode: str = "r",
        *,
        format: int | None = None,
        check: int = -1,
        preset: int | None = None,
        filters: _FilterChain | None = None,
    ) -> None:
        """Open an LZMA-compressed file in binary mode.

        filename can be either an actual file name (given as a str,
        bytes, or PathLike object), in which case the named file is
        opened, or it can be an existing file object to read from or
        write to.

        mode can be "r" for reading (default), "w" for (over)writing,
        "x" for creating exclusively, or "a" for appending. These can
        equivalently be given as "rb", "wb", "xb" and "ab" respectively.

        format specifies the container format to use for the file.
        If mode is "r", this defaults to FORMAT_AUTO. Otherwise, the
        default is FORMAT_XZ.

        check specifies the integrity check to use. This argument can
        only be used when opening a file for writing. For FORMAT_XZ,
        the default is CHECK_CRC64. FORMAT_ALONE and FORMAT_RAW do not
        support integrity checks - for these formats, check must be
        omitted, or be CHECK_NONE.

        When opening a file for reading, the *preset* argument is not
        meaningful, and should be omitted. The *filters* argument should
        also be omitted, except when format is FORMAT_RAW (in which case
        it is required).

        When opening a file for writing, the settings used by the
        compressor can be specified either as a preset compression
        level (with the *preset* argument), or in detail as a custom
        filter chain (with the *filters* argument). For FORMAT_XZ and
        FORMAT_ALONE, the default is to use the PRESET_DEFAULT preset
        level. For FORMAT_RAW, the caller must always specify a filter
        chain; the raw compressor does not support preset compression
        levels.

        preset (if provided) should be an integer in the range 0-9,
        optionally OR-ed with the constant PRESET_EXTREME.

        filters (if provided) should be a sequence of dicts. Each dict
        should have an entry for "id" indicating ID of the filter, plus
        additional entries for options to the filter.
        """

    def __enter__(self) -> Self: ...
    def peek(self, size: int = -1) -> bytes:
        """Return buffered data without advancing the file position.

        Always returns at least one byte of data, unless at EOF.
        The exact number of bytes returned is unspecified.
        """

    def read(self, size: int | None = -1) -> bytes:
        """Read up to size uncompressed bytes from the file.

        If size is negative or omitted, read until EOF is reached.
        Returns b"" if the file is already at EOF.
        """

    def read1(self, size: int = -1) -> bytes:
        """Read up to size uncompressed bytes, while trying to avoid
        making multiple reads from the underlying stream. Reads up to a
        buffer's worth of data if size is negative.

        Returns b"" if the file is at EOF.
        """

    def readline(self, size: int | None = -1) -> bytes:
        """Read a line of uncompressed bytes from the file.

        The terminating newline (if present) is retained. If size is
        non-negative, no more than size bytes will be read (in which
        case the line may be incomplete). Returns b'' if already at EOF.
        """

    def write(self, data: ReadableBuffer) -> int:
        """Write a bytes object to the file.

        Returns the number of uncompressed bytes written, which is
        always the length of data in bytes. Note that due to buffering,
        the file on disk may not reflect the data written until close()
        is called.
        """

    def seek(self, offset: int, whence: int = 0) -> int:
        """Change the file position.

        The new position is specified by offset, relative to the
        position indicated by whence. Possible values for whence are:

            0: start of stream (default): offset must not be negative
            1: current stream position
            2: end of stream; offset must not be positive

        Returns the new file position.

        Note that seeking is emulated, so depending on the parameters,
        this operation may be extremely slow.
        """

@overload
def open(
    filename: _PathOrFile,
    mode: Literal["r", "rb"] = "rb",
    *,
    format: int | None = None,
    check: Literal[-1] = -1,
    preset: None = None,
    filters: _FilterChain | None = None,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> LZMAFile:
    """Open an LZMA-compressed file in binary or text mode.

    filename can be either an actual file name (given as a str, bytes,
    or PathLike object), in which case the named file is opened, or it
    can be an existing file object to read from or write to.

    The mode argument can be "r", "rb" (default), "w", "wb", "x", "xb",
    "a", or "ab" for binary mode, or "rt", "wt", "xt", or "at" for text
    mode.

    The format, check, preset and filters arguments specify the
    compression settings, as for LZMACompressor, LZMADecompressor and
    LZMAFile.

    For binary mode, this function is equivalent to the LZMAFile
    constructor: LZMAFile(filename, mode, ...). In this case, the
    encoding, errors and newline arguments must not be provided.

    For text mode, an LZMAFile object is created, and wrapped in an
    io.TextIOWrapper instance with the specified encoding, error
    handling behavior, and line ending(s).

    """

@overload
def open(
    filename: _PathOrFile,
    mode: _OpenBinaryWritingMode,
    *,
    format: int | None = None,
    check: int = -1,
    preset: int | None = None,
    filters: _FilterChain | None = None,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> LZMAFile: ...
@overload
def open(
    filename: StrOrBytesPath,
    mode: Literal["rt"],
    *,
    format: int | None = None,
    check: Literal[-1] = -1,
    preset: None = None,
    filters: _FilterChain | None = None,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: StrOrBytesPath,
    mode: _OpenTextWritingMode,
    *,
    format: int | None = None,
    check: int = -1,
    preset: int | None = None,
    filters: _FilterChain | None = None,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: _PathOrFile,
    mode: str,
    *,
    format: int | None = None,
    check: int = -1,
    preset: int | None = None,
    filters: _FilterChain | None = None,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> LZMAFile | TextIOWrapper: ...
def compress(
    data: ReadableBuffer, format: int = 1, check: int = -1, preset: int | None = None, filters: _FilterChain | None = None
) -> bytes:
    """Compress a block of data.

    Refer to LZMACompressor's docstring for a description of the
    optional arguments *format*, *check*, *preset* and *filters*.

    For incremental compression, use an LZMACompressor instead.
    """

def decompress(data: ReadableBuffer, format: int = 0, memlimit: int | None = None, filters: _FilterChain | None = None) -> bytes:
    """Decompress a block of data.

    Refer to LZMADecompressor's docstring for a description of the
    optional arguments *format*, *check* and *filters*.

    For incremental decompression, use an LZMADecompressor instead.
    """

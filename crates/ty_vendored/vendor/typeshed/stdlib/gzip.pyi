"""Functions that read and write gzipped files.

The user of the file doesn't have to worry about the compression,
but random access is not allowed.
"""

import sys
import zlib
from _typeshed import ReadableBuffer, SizedBuffer, StrOrBytesPath, WriteableBuffer
from io import FileIO, TextIOWrapper
from typing import Final, Literal, Protocol, overload, type_check_only
from typing_extensions import TypeAlias, deprecated

if sys.version_info >= (3, 14):
    from compression._common._streams import BaseStream, DecompressReader
else:
    from _compression import BaseStream, DecompressReader

__all__ = ["BadGzipFile", "GzipFile", "open", "compress", "decompress"]

_ReadBinaryMode: TypeAlias = Literal["r", "rb"]
_WriteBinaryMode: TypeAlias = Literal["a", "ab", "w", "wb", "x", "xb"]
_OpenTextMode: TypeAlias = Literal["rt", "at", "wt", "xt"]

READ: Final[object]  # undocumented
WRITE: Final[object]  # undocumented

FTEXT: Final[int]  # actually Literal[1] # undocumented
FHCRC: Final[int]  # actually Literal[2] # undocumented
FEXTRA: Final[int]  # actually Literal[4] # undocumented
FNAME: Final[int]  # actually Literal[8] # undocumented
FCOMMENT: Final[int]  # actually Literal[16] # undocumented

@type_check_only
class _ReadableFileobj(Protocol):
    def read(self, n: int, /) -> bytes: ...
    def seek(self, n: int, /) -> object: ...
    # The following attributes and methods are optional:
    # name: str
    # mode: str
    # def fileno() -> int: ...

@type_check_only
class _WritableFileobj(Protocol):
    def write(self, b: bytes, /) -> object: ...
    def flush(self) -> object: ...
    # The following attributes and methods are optional:
    # name: str
    # mode: str
    # def fileno() -> int: ...

@overload
def open(
    filename: StrOrBytesPath | _ReadableFileobj,
    mode: _ReadBinaryMode = "rb",
    compresslevel: int = 9,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> GzipFile:
    """Open a gzip-compressed file in binary or text mode.

    The filename argument can be an actual filename (a str or bytes object), or
    an existing file object to read from or write to.

    The mode argument can be "r", "rb", "w", "wb", "x", "xb", "a" or "ab" for
    binary mode, or "rt", "wt", "xt" or "at" for text mode. The default mode is
    "rb", and the default compresslevel is 9.

    For binary mode, this function is equivalent to the GzipFile constructor:
    GzipFile(filename, mode, compresslevel). In this case, the encoding, errors
    and newline arguments must not be provided.

    For text mode, a GzipFile object is created, and wrapped in an
    io.TextIOWrapper instance with the specified encoding, error handling
    behavior, and line ending(s).

    """

@overload
def open(
    filename: StrOrBytesPath | _WritableFileobj,
    mode: _WriteBinaryMode,
    compresslevel: int = 9,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> GzipFile: ...
@overload
def open(
    filename: StrOrBytesPath | _ReadableFileobj | _WritableFileobj,
    mode: _OpenTextMode,
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
) -> GzipFile | TextIOWrapper: ...

class _PaddedFile:
    """Minimal read-only file object that prepends a string to the contents
    of an actual file. Shouldn't be used outside of gzip.py, as it lacks
    essential functionality.
    """

    file: _ReadableFileobj
    def __init__(self, f: _ReadableFileobj, prepend: bytes = b"") -> None: ...
    def read(self, size: int) -> bytes: ...
    def prepend(self, prepend: bytes = b"") -> None: ...
    def seek(self, off: int) -> int: ...
    def seekable(self) -> bool: ...

class BadGzipFile(OSError):
    """Exception raised in some cases for invalid gzip files."""

class GzipFile(BaseStream):
    """The GzipFile class simulates most of the methods of a file object with
    the exception of the truncate() method.

    This class only supports opening files in binary mode. If you need to open a
    compressed file in text mode, use the gzip.open() function.

    """

    myfileobj: FileIO | None
    mode: object
    name: str
    compress: zlib._Compress
    fileobj: _ReadableFileobj | _WritableFileobj
    @overload
    def __init__(
        self,
        filename: StrOrBytesPath | None,
        mode: _ReadBinaryMode,
        compresslevel: int = 9,
        fileobj: _ReadableFileobj | None = None,
        mtime: float | None = None,
    ) -> None:
        """Constructor for the GzipFile class.

        At least one of fileobj and filename must be given a
        non-trivial value.

        The new class instance is based on fileobj, which can be a regular
        file, an io.BytesIO object, or any other object which simulates a file.
        It defaults to None, in which case filename is opened to provide
        a file object.

        When fileobj is not None, the filename argument is only used to be
        included in the gzip file header, which may include the original
        filename of the uncompressed file.  It defaults to the filename of
        fileobj, if discernible; otherwise, it defaults to the empty string,
        and in this case the original filename is not included in the header.

        The mode argument can be any of 'r', 'rb', 'a', 'ab', 'w', 'wb', 'x', or
        'xb' depending on whether the file will be read or written.  The default
        is the mode of fileobj if discernible; otherwise, the default is 'rb'.
        A mode of 'r' is equivalent to one of 'rb', and similarly for 'w' and
        'wb', 'a' and 'ab', and 'x' and 'xb'.

        The compresslevel argument is an integer from 0 to 9 controlling the
        level of compression; 1 is fastest and produces the least compression,
        and 9 is slowest and produces the most compression. 0 is no compression
        at all. The default is 9.

        The optional mtime argument is the timestamp requested by gzip. The time
        is in Unix format, i.e., seconds since 00:00:00 UTC, January 1, 1970.
        If mtime is omitted or None, the current time is used. Use mtime = 0
        to generate a compressed stream that does not depend on creation time.

        """

    @overload
    def __init__(
        self,
        *,
        mode: _ReadBinaryMode,
        compresslevel: int = 9,
        fileobj: _ReadableFileobj | None = None,
        mtime: float | None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        filename: StrOrBytesPath | None,
        mode: _WriteBinaryMode,
        compresslevel: int = 9,
        fileobj: _WritableFileobj | None = None,
        mtime: float | None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        *,
        mode: _WriteBinaryMode,
        compresslevel: int = 9,
        fileobj: _WritableFileobj | None = None,
        mtime: float | None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        filename: StrOrBytesPath | None = None,
        mode: str | None = None,
        compresslevel: int = 9,
        fileobj: _ReadableFileobj | _WritableFileobj | None = None,
        mtime: float | None = None,
    ) -> None: ...
    if sys.version_info < (3, 12):
        @property
        @deprecated("Deprecated since Python 2.6; removed in Python 3.12. Use `name` attribute instead.")
        def filename(self) -> str: ...

    @property
    def mtime(self) -> int | None:
        """Last modification time read from stream, or None"""
    crc: int
    def write(self, data: ReadableBuffer) -> int: ...
    def read(self, size: int | None = -1) -> bytes: ...
    def read1(self, size: int = -1) -> bytes:
        """Implements BufferedIOBase.read1()

        Reads up to a buffer's worth of data if size is negative.
        """

    def peek(self, n: int) -> bytes: ...
    def close(self) -> None: ...
    def flush(self, zlib_mode: int = 2) -> None: ...
    def fileno(self) -> int:
        """Invoke the underlying file object's fileno() method.

        This will raise AttributeError if the underlying file object
        doesn't support fileno().
        """

    def rewind(self) -> None:
        """Return the uncompressed stream file position indicator to the
        beginning of the file
        """

    def seek(self, offset: int, whence: int = 0) -> int: ...
    def readline(self, size: int | None = -1) -> bytes: ...

    if sys.version_info >= (3, 14):
        def readinto(self, b: WriteableBuffer) -> int: ...
        def readinto1(self, b: WriteableBuffer) -> int: ...

class _GzipReader(DecompressReader):
    def __init__(self, fp: _ReadableFileobj) -> None: ...

if sys.version_info >= (3, 14):
    def compress(data: SizedBuffer, compresslevel: int = 9, *, mtime: float = 0) -> bytes:
        """Compress data in one shot and return the compressed string.

        compresslevel sets the compression level in range of 0-9.
        mtime can be used to set the modification time.
        The modification time is set to 0 by default, for reproducibility.
        """

else:
    def compress(data: SizedBuffer, compresslevel: int = 9, *, mtime: float | None = None) -> bytes:
        """Compress data in one shot and return the compressed string.

        compresslevel sets the compression level in range of 0-9.
        mtime can be used to set the modification time. The modification time is
        set to the current time by default.
        """

def decompress(data: ReadableBuffer) -> bytes:
    """Decompress a gzip compressed string in one shot.
    Return the decompressed string.
    """

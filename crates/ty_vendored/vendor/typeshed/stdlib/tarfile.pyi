"""Read from and write to tar format archives."""

import bz2
import io
import sys
from _typeshed import ReadableBuffer, StrOrBytesPath, StrPath, SupportsRead, WriteableBuffer
from builtins import list as _list  # aliases to avoid name clashes with fields named "type" or "list"
from collections.abc import Callable, Iterable, Iterator, Mapping
from gzip import _ReadableFileobj as _GzipReadableFileobj, _WritableFileobj as _GzipWritableFileobj
from types import TracebackType
from typing import IO, ClassVar, Final, Literal, Protocol, overload, type_check_only
from typing_extensions import Self, TypeAlias, deprecated

if sys.version_info >= (3, 14):
    from compression.zstd import ZstdDict

__all__ = [
    "TarFile",
    "TarInfo",
    "is_tarfile",
    "TarError",
    "ReadError",
    "CompressionError",
    "StreamError",
    "ExtractError",
    "HeaderError",
    "ENCODING",
    "USTAR_FORMAT",
    "GNU_FORMAT",
    "PAX_FORMAT",
    "DEFAULT_FORMAT",
    "open",
]
if sys.version_info >= (3, 12):
    __all__ += [
        "fully_trusted_filter",
        "data_filter",
        "tar_filter",
        "FilterError",
        "AbsoluteLinkError",
        "OutsideDestinationError",
        "SpecialFileError",
        "AbsolutePathError",
        "LinkOutsideDestinationError",
    ]
if sys.version_info >= (3, 13):
    __all__ += ["LinkFallbackError"]

_FilterFunction: TypeAlias = Callable[[TarInfo, str], TarInfo | None]
_TarfileFilter: TypeAlias = Literal["fully_trusted", "tar", "data"] | _FilterFunction

@type_check_only
class _Fileobj(Protocol):
    def read(self, size: int, /) -> bytes: ...
    def write(self, b: bytes, /) -> object: ...
    def tell(self) -> int: ...
    def seek(self, pos: int, /) -> object: ...
    def close(self) -> object: ...
    # Optional fields:
    # name: str | bytes
    # mode: Literal["rb", "r+b", "wb", "xb"]

@type_check_only
class _Bz2ReadableFileobj(bz2._ReadableFileobj):
    def close(self) -> object: ...

@type_check_only
class _Bz2WritableFileobj(bz2._WritableFileobj):
    def close(self) -> object: ...

# tar constants
NUL: Final = b"\0"
BLOCKSIZE: Final = 512
RECORDSIZE: Final = 10240
GNU_MAGIC: Final = b"ustar  \0"
POSIX_MAGIC: Final = b"ustar\x0000"

LENGTH_NAME: Final = 100
LENGTH_LINK: Final = 100
LENGTH_PREFIX: Final = 155

REGTYPE: Final = b"0"
AREGTYPE: Final = b"\0"
LNKTYPE: Final = b"1"
SYMTYPE: Final = b"2"
CHRTYPE: Final = b"3"
BLKTYPE: Final = b"4"
DIRTYPE: Final = b"5"
FIFOTYPE: Final = b"6"
CONTTYPE: Final = b"7"

GNUTYPE_LONGNAME: Final = b"L"
GNUTYPE_LONGLINK: Final = b"K"
GNUTYPE_SPARSE: Final = b"S"

XHDTYPE: Final = b"x"
XGLTYPE: Final = b"g"
SOLARIS_XHDTYPE: Final = b"X"

_TarFormat: TypeAlias = Literal[0, 1, 2]  # does not exist at runtime
USTAR_FORMAT: Final = 0
GNU_FORMAT: Final = 1
PAX_FORMAT: Final = 2
DEFAULT_FORMAT: Final = PAX_FORMAT

# tarfile constants

SUPPORTED_TYPES: Final[tuple[bytes, ...]]
REGULAR_TYPES: Final[tuple[bytes, ...]]
GNU_TYPES: Final[tuple[bytes, ...]]
PAX_FIELDS: Final[tuple[str, ...]]
PAX_NUMBER_FIELDS: Final[dict[str, type]]
PAX_NAME_FIELDS: Final[set[str]]

ENCODING: Final[str]

class ExFileObject(io.BufferedReader):  # undocumented
    def __init__(self, tarfile: TarFile, tarinfo: TarInfo) -> None: ...

class TarFile:
    """The TarFile Class provides an interface to tar archives."""

    OPEN_METH: ClassVar[Mapping[str, str]]
    name: StrOrBytesPath | None
    mode: Literal["r", "a", "w", "x"]
    fileobj: _Fileobj | None
    format: _TarFormat | None
    tarinfo: type[TarInfo]
    dereference: bool | None
    ignore_zeros: bool | None
    encoding: str | None
    errors: str
    fileobject: type[ExFileObject]  # undocumented
    pax_headers: Mapping[str, str] | None
    debug: int | None
    errorlevel: int | None
    offset: int  # undocumented
    extraction_filter: _FilterFunction | None
    if sys.version_info >= (3, 13):
        stream: bool
        def __init__(
            self,
            name: StrOrBytesPath | None = None,
            mode: Literal["r", "a", "w", "x"] = "r",
            fileobj: _Fileobj | None = None,
            format: int | None = None,
            tarinfo: type[TarInfo] | None = None,
            dereference: bool | None = None,
            ignore_zeros: bool | None = None,
            encoding: str | None = None,
            errors: str = "surrogateescape",
            pax_headers: Mapping[str, str] | None = None,
            debug: int | None = None,
            errorlevel: int | None = None,
            copybufsize: int | None = None,  # undocumented
            stream: bool = False,
        ) -> None:
            """Open an (uncompressed) tar archive 'name'. 'mode' is either 'r' to
            read from an existing archive, 'a' to append data to an existing
            file or 'w' to create a new file overwriting an existing one. 'mode'
            defaults to 'r'.
            If 'fileobj' is given, it is used for reading or writing data. If it
            can be determined, 'mode' is overridden by 'fileobj's mode.
            'fileobj' is not closed, when TarFile is closed.
            """
    else:
        def __init__(
            self,
            name: StrOrBytesPath | None = None,
            mode: Literal["r", "a", "w", "x"] = "r",
            fileobj: _Fileobj | None = None,
            format: int | None = None,
            tarinfo: type[TarInfo] | None = None,
            dereference: bool | None = None,
            ignore_zeros: bool | None = None,
            encoding: str | None = None,
            errors: str = "surrogateescape",
            pax_headers: Mapping[str, str] | None = None,
            debug: int | None = None,
            errorlevel: int | None = None,
            copybufsize: int | None = None,  # undocumented
        ) -> None:
            """Open an (uncompressed) tar archive `name'. `mode' is either 'r' to
            read from an existing archive, 'a' to append data to an existing
            file or 'w' to create a new file overwriting an existing one. `mode'
            defaults to 'r'.
            If `fileobj' is given, it is used for reading or writing data. If it
            can be determined, `mode' is overridden by `fileobj's mode.
            `fileobj' is not closed, when TarFile is closed.
            """

    def __enter__(self) -> Self: ...
    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
    ) -> None: ...
    def __iter__(self) -> Iterator[TarInfo]:
        """Provide an iterator object."""

    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | None = None,
        mode: Literal["r", "r:*", "r:", "r:gz", "r:bz2", "r:xz"] = "r",
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self:
        """Open a tar archive for reading, writing or appending. Return
        an appropriate TarFile class.

        mode:
        'r' or 'r:*' open for reading with transparent compression
        'r:'         open for reading exclusively uncompressed
        'r:gz'       open for reading with gzip compression
        'r:bz2'      open for reading with bzip2 compression
        'r:xz'       open for reading with lzma compression
        'r:zst'      open for reading with zstd compression
        'a' or 'a:'  open for appending, creating the file if necessary
        'w' or 'w:'  open for writing without compression
        'w:gz'       open for writing with gzip compression
        'w:bz2'      open for writing with bzip2 compression
        'w:xz'       open for writing with lzma compression
        'w:zst'      open for writing with zstd compression

        'x' or 'x:'  create a tarfile exclusively without compression, raise
                     an exception if the file is already created
        'x:gz'       create a gzip compressed tarfile, raise an exception
                     if the file is already created
        'x:bz2'      create a bzip2 compressed tarfile, raise an exception
                     if the file is already created
        'x:xz'       create an lzma compressed tarfile, raise an exception
                     if the file is already created
        'x:zst'      create a zstd compressed tarfile, raise an exception
                     if the file is already created

        'r|*'        open a stream of tar blocks with transparent compression
        'r|'         open an uncompressed stream of tar blocks for reading
        'r|gz'       open a gzip compressed stream of tar blocks
        'r|bz2'      open a bzip2 compressed stream of tar blocks
        'r|xz'       open an lzma compressed stream of tar blocks
        'r|zst'      open a zstd compressed stream of tar blocks
        'w|'         open an uncompressed stream for writing
        'w|gz'       open a gzip compressed stream for writing
        'w|bz2'      open a bzip2 compressed stream for writing
        'w|xz'       open an lzma compressed stream for writing
        'w|zst'      open a zstd compressed stream for writing
        """
    if sys.version_info >= (3, 14):
        @overload
        @classmethod
        def open(
            cls,
            name: StrOrBytesPath | None,
            mode: Literal["r:zst"],
            fileobj: _Fileobj | None = None,
            bufsize: int = 10240,
            *,
            format: int | None = ...,
            tarinfo: type[TarInfo] | None = ...,
            dereference: bool | None = ...,
            ignore_zeros: bool | None = ...,
            encoding: str | None = ...,
            errors: str = ...,
            pax_headers: Mapping[str, str] | None = ...,
            debug: int | None = ...,
            errorlevel: int | None = ...,
            level: None = None,
            options: Mapping[int, int] | None = None,
            zstd_dict: ZstdDict | tuple[ZstdDict, int] | None = None,
        ) -> Self:
            """Open a tar archive for reading, writing or appending. Return
            an appropriate TarFile class.

            mode:
            'r' or 'r:*' open for reading with transparent compression
            'r:'         open for reading exclusively uncompressed
            'r:gz'       open for reading with gzip compression
            'r:bz2'      open for reading with bzip2 compression
            'r:xz'       open for reading with lzma compression
            'r:zst'      open for reading with zstd compression
            'a' or 'a:'  open for appending, creating the file if necessary
            'w' or 'w:'  open for writing without compression
            'w:gz'       open for writing with gzip compression
            'w:bz2'      open for writing with bzip2 compression
            'w:xz'       open for writing with lzma compression
            'w:zst'      open for writing with zstd compression

            'x' or 'x:'  create a tarfile exclusively without compression, raise
                         an exception if the file is already created
            'x:gz'       create a gzip compressed tarfile, raise an exception
                         if the file is already created
            'x:bz2'      create a bzip2 compressed tarfile, raise an exception
                         if the file is already created
            'x:xz'       create an lzma compressed tarfile, raise an exception
                         if the file is already created
            'x:zst'      create a zstd compressed tarfile, raise an exception
                         if the file is already created

            'r|*'        open a stream of tar blocks with transparent compression
            'r|'         open an uncompressed stream of tar blocks for reading
            'r|gz'       open a gzip compressed stream of tar blocks
            'r|bz2'      open a bzip2 compressed stream of tar blocks
            'r|xz'       open an lzma compressed stream of tar blocks
            'r|zst'      open a zstd compressed stream of tar blocks
            'w|'         open an uncompressed stream for writing
            'w|gz'       open a gzip compressed stream for writing
            'w|bz2'      open a bzip2 compressed stream for writing
            'w|xz'       open an lzma compressed stream for writing
            'w|zst'      open a zstd compressed stream for writing
            """

    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | None,
        mode: Literal["x", "x:", "a", "a:", "w", "w:", "w:tar"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | None = None,
        *,
        mode: Literal["x", "x:", "a", "a:", "w", "w:", "w:tar"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | None,
        mode: Literal["x:gz", "x:bz2", "w:gz", "w:bz2"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
        compresslevel: int = 9,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | None = None,
        *,
        mode: Literal["x:gz", "x:bz2", "w:gz", "w:bz2"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
        compresslevel: int = 9,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | None,
        mode: Literal["x:xz", "w:xz"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
        preset: Literal[0, 1, 2, 3, 4, 5, 6, 7, 8, 9] | None = ...,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | None = None,
        *,
        mode: Literal["x:xz", "w:xz"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
        preset: Literal[0, 1, 2, 3, 4, 5, 6, 7, 8, 9] | None = ...,
    ) -> Self: ...
    if sys.version_info >= (3, 14):
        @overload
        @classmethod
        def open(
            cls,
            name: StrOrBytesPath | None,
            mode: Literal["x:zst", "w:zst"],
            fileobj: _Fileobj | None = None,
            bufsize: int = 10240,
            *,
            format: int | None = ...,
            tarinfo: type[TarInfo] | None = ...,
            dereference: bool | None = ...,
            ignore_zeros: bool | None = ...,
            encoding: str | None = ...,
            errors: str = ...,
            pax_headers: Mapping[str, str] | None = ...,
            debug: int | None = ...,
            errorlevel: int | None = ...,
            options: Mapping[int, int] | None = None,
            zstd_dict: ZstdDict | tuple[ZstdDict, int] | None = None,
        ) -> Self:
            """Open a tar archive for reading, writing or appending. Return
            an appropriate TarFile class.

            mode:
            'r' or 'r:*' open for reading with transparent compression
            'r:'         open for reading exclusively uncompressed
            'r:gz'       open for reading with gzip compression
            'r:bz2'      open for reading with bzip2 compression
            'r:xz'       open for reading with lzma compression
            'r:zst'      open for reading with zstd compression
            'a' or 'a:'  open for appending, creating the file if necessary
            'w' or 'w:'  open for writing without compression
            'w:gz'       open for writing with gzip compression
            'w:bz2'      open for writing with bzip2 compression
            'w:xz'       open for writing with lzma compression
            'w:zst'      open for writing with zstd compression

            'x' or 'x:'  create a tarfile exclusively without compression, raise
                         an exception if the file is already created
            'x:gz'       create a gzip compressed tarfile, raise an exception
                         if the file is already created
            'x:bz2'      create a bzip2 compressed tarfile, raise an exception
                         if the file is already created
            'x:xz'       create an lzma compressed tarfile, raise an exception
                         if the file is already created
            'x:zst'      create a zstd compressed tarfile, raise an exception
                         if the file is already created

            'r|*'        open a stream of tar blocks with transparent compression
            'r|'         open an uncompressed stream of tar blocks for reading
            'r|gz'       open a gzip compressed stream of tar blocks
            'r|bz2'      open a bzip2 compressed stream of tar blocks
            'r|xz'       open an lzma compressed stream of tar blocks
            'r|zst'      open a zstd compressed stream of tar blocks
            'w|'         open an uncompressed stream for writing
            'w|gz'       open a gzip compressed stream for writing
            'w|bz2'      open a bzip2 compressed stream for writing
            'w|xz'       open an lzma compressed stream for writing
            'w|zst'      open a zstd compressed stream for writing
            """

        @overload
        @classmethod
        def open(
            cls,
            name: StrOrBytesPath | None = None,
            *,
            mode: Literal["x:zst", "w:zst"],
            fileobj: _Fileobj | None = None,
            bufsize: int = 10240,
            format: int | None = ...,
            tarinfo: type[TarInfo] | None = ...,
            dereference: bool | None = ...,
            ignore_zeros: bool | None = ...,
            encoding: str | None = ...,
            errors: str = ...,
            pax_headers: Mapping[str, str] | None = ...,
            debug: int | None = ...,
            errorlevel: int | None = ...,
            options: Mapping[int, int] | None = None,
            zstd_dict: ZstdDict | tuple[ZstdDict, int] | None = None,
        ) -> Self: ...

    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | ReadableBuffer | None,
        mode: Literal["r|*", "r|", "r|gz", "r|bz2", "r|xz", "r|zst"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | ReadableBuffer | None = None,
        *,
        mode: Literal["r|*", "r|", "r|gz", "r|bz2", "r|xz", "r|zst"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | WriteableBuffer | None,
        mode: Literal["w|", "w|xz", "w|zst"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | WriteableBuffer | None = None,
        *,
        mode: Literal["w|", "w|xz", "w|zst"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | WriteableBuffer | None,
        mode: Literal["w|gz", "w|bz2"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
        compresslevel: int = 9,
    ) -> Self: ...
    @overload
    @classmethod
    def open(
        cls,
        name: StrOrBytesPath | WriteableBuffer | None = None,
        *,
        mode: Literal["w|gz", "w|bz2"],
        fileobj: _Fileobj | None = None,
        bufsize: int = 10240,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        errors: str = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
        compresslevel: int = 9,
    ) -> Self: ...
    @classmethod
    def taropen(
        cls,
        name: StrOrBytesPath | None,
        mode: Literal["r", "a", "w", "x"] = "r",
        fileobj: _Fileobj | None = None,
        *,
        compresslevel: int = ...,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self:
        """Open uncompressed tar archive name for reading or writing."""

    @overload
    @classmethod
    def gzopen(
        cls,
        name: StrOrBytesPath | None,
        mode: Literal["r"] = "r",
        fileobj: _GzipReadableFileobj | None = None,
        compresslevel: int = 9,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self:
        """Open gzip compressed tar archive name for reading or writing.
        Appending is not allowed.
        """

    @overload
    @classmethod
    def gzopen(
        cls,
        name: StrOrBytesPath | None,
        mode: Literal["w", "x"],
        fileobj: _GzipWritableFileobj | None = None,
        compresslevel: int = 9,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self: ...
    @overload
    @classmethod
    def bz2open(
        cls,
        name: StrOrBytesPath | None,
        mode: Literal["w", "x"],
        fileobj: _Bz2WritableFileobj | None = None,
        compresslevel: int = 9,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self:
        """Open bzip2 compressed tar archive name for reading or writing.
        Appending is not allowed.
        """

    @overload
    @classmethod
    def bz2open(
        cls,
        name: StrOrBytesPath | None,
        mode: Literal["r"] = "r",
        fileobj: _Bz2ReadableFileobj | None = None,
        compresslevel: int = 9,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self: ...
    @classmethod
    def xzopen(
        cls,
        name: StrOrBytesPath | None,
        mode: Literal["r", "w", "x"] = "r",
        fileobj: IO[bytes] | None = None,
        preset: int | None = None,
        *,
        format: int | None = ...,
        tarinfo: type[TarInfo] | None = ...,
        dereference: bool | None = ...,
        ignore_zeros: bool | None = ...,
        encoding: str | None = ...,
        pax_headers: Mapping[str, str] | None = ...,
        debug: int | None = ...,
        errorlevel: int | None = ...,
    ) -> Self:
        """Open lzma compressed tar archive name for reading or writing.
        Appending is not allowed.
        """
    if sys.version_info >= (3, 14):
        @overload
        @classmethod
        def zstopen(
            cls,
            name: StrOrBytesPath | None,
            mode: Literal["r"] = "r",
            fileobj: IO[bytes] | None = None,
            level: None = None,
            options: Mapping[int, int] | None = None,
            zstd_dict: ZstdDict | tuple[ZstdDict, int] | None = None,
            *,
            format: int | None = ...,
            tarinfo: type[TarInfo] | None = ...,
            dereference: bool | None = ...,
            ignore_zeros: bool | None = ...,
            encoding: str | None = ...,
            pax_headers: Mapping[str, str] | None = ...,
            debug: int | None = ...,
            errorlevel: int | None = ...,
        ) -> Self:
            """Open zstd compressed tar archive name for reading or writing.
            Appending is not allowed.
            """

        @overload
        @classmethod
        def zstopen(
            cls,
            name: StrOrBytesPath | None,
            mode: Literal["w", "x"],
            fileobj: IO[bytes] | None = None,
            level: int | None = None,
            options: Mapping[int, int] | None = None,
            zstd_dict: ZstdDict | tuple[ZstdDict, int] | None = None,
            *,
            format: int | None = ...,
            tarinfo: type[TarInfo] | None = ...,
            dereference: bool | None = ...,
            ignore_zeros: bool | None = ...,
            encoding: str | None = ...,
            pax_headers: Mapping[str, str] | None = ...,
            debug: int | None = ...,
            errorlevel: int | None = ...,
        ) -> Self: ...

    def getmember(self, name: str) -> TarInfo:
        """Return a TarInfo object for member 'name'. If 'name' can not be
        found in the archive, KeyError is raised. If a member occurs more
        than once in the archive, its last occurrence is assumed to be the
        most up-to-date version.
        """

    def getmembers(self) -> _list[TarInfo]:
        """Return the members of the archive as a list of TarInfo objects. The
        list has the same order as the members in the archive.
        """

    def getnames(self) -> _list[str]:
        """Return the members of the archive as a list of their names. It has
        the same order as the list returned by getmembers().
        """

    def list(self, verbose: bool = True, *, members: Iterable[TarInfo] | None = None) -> None:
        """Print a table of contents to sys.stdout. If 'verbose' is False, only
        the names of the members are printed. If it is True, an 'ls -l'-like
        output is produced. 'members' is optional and must be a subset of the
        list returned by getmembers().
        """

    def next(self) -> TarInfo | None:
        """Return the next member of the archive as a TarInfo object, when
        TarFile is opened for reading. Return None if there is no more
        available.
        """
    # Calling this method without `filter` is deprecated, but it may be set either on the class or in an
    # individual call, so we can't mark it as @deprecated here.
    def extractall(
        self,
        path: StrOrBytesPath = ".",
        members: Iterable[TarInfo] | None = None,
        *,
        numeric_owner: bool = False,
        filter: _TarfileFilter | None = None,
    ) -> None:
        """Extract all members from the archive to the current working
        directory and set owner, modification time and permissions on
        directories afterwards. 'path' specifies a different directory
        to extract to. 'members' is optional and must be a subset of the
        list returned by getmembers(). If 'numeric_owner' is True, only
        the numbers for user/group names are used and not the names.

        The 'filter' function will be called on each member just
        before extraction.
        It can return a changed TarInfo or None to skip the member.
        String names of common filters are accepted.
        """
    # Same situation as for `extractall`.
    def extract(
        self,
        member: str | TarInfo,
        path: StrOrBytesPath = "",
        set_attrs: bool = True,
        *,
        numeric_owner: bool = False,
        filter: _TarfileFilter | None = None,
    ) -> None:
        """Extract a member from the archive to the current working directory,
        using its full name. Its file information is extracted as accurately
        as possible. 'member' may be a filename or a TarInfo object. You can
        specify a different directory using 'path'. File attributes (owner,
        mtime, mode) are set unless 'set_attrs' is False. If 'numeric_owner'
        is True, only the numbers for user/group names are used and not
        the names.

        The 'filter' function will be called before extraction.
        It can return a changed TarInfo or None to skip the member.
        String names of common filters are accepted.
        """

    def _extract_member(
        self,
        tarinfo: TarInfo,
        targetpath: str,
        set_attrs: bool = True,
        numeric_owner: bool = False,
        *,
        filter_function: _FilterFunction | None = None,
        extraction_root: str | None = None,
    ) -> None:  # undocumented
        """Extract the filtered TarInfo object tarinfo to a physical
        file called targetpath.

        filter_function is only used when extracting a *different*
        member (e.g. as fallback to creating a symlink)
        """

    def extractfile(self, member: str | TarInfo) -> IO[bytes] | None:
        """Extract a member from the archive as a file object. 'member' may be
        a filename or a TarInfo object. If 'member' is a regular file or
        a link, an io.BufferedReader object is returned. For all other
        existing members, None is returned. If 'member' does not appear
        in the archive, KeyError is raised.
        """

    def makedir(self, tarinfo: TarInfo, targetpath: StrOrBytesPath) -> None:  # undocumented
        """Make a directory called targetpath."""

    def makefile(self, tarinfo: TarInfo, targetpath: StrOrBytesPath) -> None:  # undocumented
        """Make a file called targetpath."""

    def makeunknown(self, tarinfo: TarInfo, targetpath: StrOrBytesPath) -> None:  # undocumented
        """Make a file from a TarInfo object with an unknown type
        at targetpath.
        """

    def makefifo(self, tarinfo: TarInfo, targetpath: StrOrBytesPath) -> None:  # undocumented
        """Make a fifo called targetpath."""

    def makedev(self, tarinfo: TarInfo, targetpath: StrOrBytesPath) -> None:  # undocumented
        """Make a character or block device called targetpath."""

    def makelink(self, tarinfo: TarInfo, targetpath: StrOrBytesPath) -> None:  # undocumented
        """Make a (symbolic) link called targetpath. If it cannot be created
        (platform limitation), we try to make a copy of the referenced file
        instead of a link.
        """

    def makelink_with_filter(
        self, tarinfo: TarInfo, targetpath: StrOrBytesPath, filter_function: _FilterFunction, extraction_root: str
    ) -> None:  # undocumented
        """Make a (symbolic) link called targetpath. If it cannot be created
        (platform limitation), we try to make a copy of the referenced file
        instead of a link.

        filter_function is only used when extracting a *different*
        member (e.g. as fallback to creating a link).
        """

    def chown(self, tarinfo: TarInfo, targetpath: StrOrBytesPath, numeric_owner: bool) -> None:  # undocumented
        """Set owner of targetpath according to tarinfo. If numeric_owner
        is True, use .gid/.uid instead of .gname/.uname. If numeric_owner
        is False, fall back to .gid/.uid when the search based on name
        fails.
        """

    def chmod(self, tarinfo: TarInfo, targetpath: StrOrBytesPath) -> None:  # undocumented
        """Set file permissions of targetpath according to tarinfo."""

    def utime(self, tarinfo: TarInfo, targetpath: StrOrBytesPath) -> None:  # undocumented
        """Set modification time of targetpath according to tarinfo."""

    def add(
        self,
        name: StrPath,
        arcname: StrPath | None = None,
        recursive: bool = True,
        *,
        filter: Callable[[TarInfo], TarInfo | None] | None = None,
    ) -> None:
        """Add the file 'name' to the archive. 'name' may be any type of file
        (directory, fifo, symbolic link, etc.). If given, 'arcname'
        specifies an alternative name for the file in the archive.
        Directories are added recursively by default. This can be avoided by
        setting 'recursive' to False. 'filter' is a function
        that expects a TarInfo object argument and returns the changed
        TarInfo object, if it returns None the TarInfo object will be
        excluded from the archive.
        """

    def addfile(self, tarinfo: TarInfo, fileobj: SupportsRead[bytes] | None = None) -> None:
        """Add the TarInfo object 'tarinfo' to the archive. If 'tarinfo' represents
        a non zero-size regular file, the 'fileobj' argument should be a binary file,
        and tarinfo.size bytes are read from it and added to the archive.
        You can create TarInfo objects directly, or by using gettarinfo().
        """

    def gettarinfo(
        self, name: StrOrBytesPath | None = None, arcname: str | None = None, fileobj: IO[bytes] | None = None
    ) -> TarInfo:
        """Create a TarInfo object from the result of os.stat or equivalent
        on an existing file. The file is either named by 'name', or
        specified as a file object 'fileobj' with a file descriptor. If
        given, 'arcname' specifies an alternative name for the file in the
        archive, otherwise, the name is taken from the 'name' attribute of
        'fileobj', or the 'name' argument. The name should be a text
        string.
        """

    def close(self) -> None:
        """Close the TarFile. In write-mode, two finishing zero blocks are
        appended to the archive.
        """

open = TarFile.open

def is_tarfile(name: StrOrBytesPath | IO[bytes]) -> bool:
    """Return True if name points to a tar archive that we
    are able to handle, else return False.

    'name' should be a string, file, or file-like object.
    """

class TarError(Exception):
    """Base exception."""

class ReadError(TarError):
    """Exception for unreadable tar archives."""

class CompressionError(TarError):
    """Exception for unavailable compression methods."""

class StreamError(TarError):
    """Exception for unsupported operations on stream-like TarFiles."""

class ExtractError(TarError):
    """General exception for extract errors."""

class HeaderError(TarError):
    """Base exception for header errors."""

class FilterError(TarError):
    # This attribute is only set directly on the subclasses, but the documentation guarantees
    # that it is always present on FilterError.
    tarinfo: TarInfo

class AbsolutePathError(FilterError):
    def __init__(self, tarinfo: TarInfo) -> None: ...

class OutsideDestinationError(FilterError):
    def __init__(self, tarinfo: TarInfo, path: str) -> None: ...

class SpecialFileError(FilterError):
    def __init__(self, tarinfo: TarInfo) -> None: ...

class AbsoluteLinkError(FilterError):
    def __init__(self, tarinfo: TarInfo) -> None: ...

class LinkOutsideDestinationError(FilterError):
    def __init__(self, tarinfo: TarInfo, path: str) -> None: ...

class LinkFallbackError(FilterError):
    def __init__(self, tarinfo: TarInfo, path: str) -> None: ...

def fully_trusted_filter(member: TarInfo, dest_path: str) -> TarInfo: ...
def tar_filter(member: TarInfo, dest_path: str) -> TarInfo: ...
def data_filter(member: TarInfo, dest_path: str) -> TarInfo: ...

class TarInfo:
    """Informational class which holds the details about an
    archive member given by a tar header block.
    TarInfo objects are returned by TarFile.getmember(),
    TarFile.getmembers() and TarFile.gettarinfo() and are
    usually created internally.
    """

    __slots__ = (
        "name",
        "mode",
        "uid",
        "gid",
        "size",
        "mtime",
        "chksum",
        "type",
        "linkname",
        "uname",
        "gname",
        "devmajor",
        "devminor",
        "offset",
        "offset_data",
        "pax_headers",
        "sparse",
        "_tarfile",
        "_sparse_structs",
        "_link_target",
    )
    name: str
    path: str
    size: int
    mtime: int | float
    chksum: int
    devmajor: int
    devminor: int
    offset: int
    offset_data: int
    sparse: bytes | None
    mode: int
    type: bytes  # usually one of the TYPE constants, but could be an arbitrary byte
    linkname: str
    uid: int
    gid: int
    uname: str
    gname: str
    pax_headers: Mapping[str, str]
    def __init__(self, name: str = "") -> None:
        """Construct a TarInfo object. name is the optional name
        of the member.
        """
    if sys.version_info >= (3, 13):
        @property
        @deprecated("Deprecated since Python 3.13; will be removed in Python 3.16.")
        def tarfile(self) -> TarFile | None: ...
        @tarfile.setter
        @deprecated("Deprecated since Python 3.13; will be removed in Python 3.16.")
        def tarfile(self, tarfile: TarFile | None) -> None: ...
    else:
        tarfile: TarFile | None

    @classmethod
    def frombuf(cls, buf: bytes | bytearray, encoding: str, errors: str) -> Self:
        """Construct a TarInfo object from a 512 byte bytes object."""

    @classmethod
    def fromtarfile(cls, tarfile: TarFile) -> Self:
        """Return the next TarInfo object from TarFile object
        tarfile.
        """

    @property
    def linkpath(self) -> str:
        """In pax headers, "linkname" is called "linkpath"."""

    @linkpath.setter
    def linkpath(self, linkname: str) -> None: ...
    def replace(
        self,
        *,
        name: str = ...,
        mtime: float = ...,
        mode: int = ...,
        linkname: str = ...,
        uid: int = ...,
        gid: int = ...,
        uname: str = ...,
        gname: str = ...,
        deep: bool = True,
    ) -> Self:
        """Return a deep copy of self with the given attributes replaced."""

    def get_info(self) -> Mapping[str, str | int | bytes | Mapping[str, str]]:
        """Return the TarInfo's attributes as a dictionary."""

    def tobuf(self, format: _TarFormat | None = 2, encoding: str | None = "utf-8", errors: str = "surrogateescape") -> bytes:
        """Return a tar header as a string of 512 byte blocks."""

    def create_ustar_header(self, info: Mapping[str, str | int | bytes | Mapping[str, str]], encoding: str, errors: str) -> bytes:
        """Return the object as a ustar header block."""

    def create_gnu_header(self, info: Mapping[str, str | int | bytes | Mapping[str, str]], encoding: str, errors: str) -> bytes:
        """Return the object as a GNU header block sequence."""

    def create_pax_header(self, info: Mapping[str, str | int | bytes | Mapping[str, str]], encoding: str) -> bytes:
        """Return the object as a ustar header block. If it cannot be
        represented this way, prepend a pax extended header sequence
        with supplement information.
        """

    @classmethod
    def create_pax_global_header(cls, pax_headers: Mapping[str, str]) -> bytes:
        """Return the object as a pax global header block sequence."""

    def isfile(self) -> bool:
        """Return True if the Tarinfo object is a regular file."""

    def isreg(self) -> bool:
        """Return True if the Tarinfo object is a regular file."""

    def issparse(self) -> bool: ...
    def isdir(self) -> bool:
        """Return True if it is a directory."""

    def issym(self) -> bool:
        """Return True if it is a symbolic link."""

    def islnk(self) -> bool:
        """Return True if it is a hard link."""

    def ischr(self) -> bool:
        """Return True if it is a character device."""

    def isblk(self) -> bool:
        """Return True if it is a block device."""

    def isfifo(self) -> bool:
        """Return True if it is a FIFO."""

    def isdev(self) -> bool:
        """Return True if it is one of character device, block device or FIFO."""

import io
import sys
from _typeshed import SizedBuffer, StrOrBytesPath, StrPath
from collections.abc import Callable, Iterable, Iterator
from io import TextIOWrapper
from os import PathLike
from types import TracebackType
from typing import IO, Final, Literal, Protocol, overload
from typing_extensions import Self, TypeAlias

__all__ = [
    "BadZipFile",
    "BadZipfile",
    "Path",
    "error",
    "ZIP_STORED",
    "ZIP_DEFLATED",
    "ZIP_BZIP2",
    "ZIP_LZMA",
    "is_zipfile",
    "ZipInfo",
    "ZipFile",
    "PyZipFile",
    "LargeZipFile",
]

if sys.version_info >= (3, 14):
    __all__ += ["ZIP_ZSTANDARD"]

# TODO: use TypeAlias for these two when mypy bugs are fixed
# https://github.com/python/mypy/issues/16581
_DateTuple = tuple[int, int, int, int, int, int]  # noqa: Y026
_ZipFileMode = Literal["r", "w", "x", "a"]  # noqa: Y026

_ReadWriteMode: TypeAlias = Literal["r", "w"]

class BadZipFile(Exception): ...

BadZipfile = BadZipFile
error = BadZipfile

class LargeZipFile(Exception): ...

class _ZipStream(Protocol):
    def read(self, n: int, /) -> bytes: ...
    # The following methods are optional:
    # def seekable(self) -> bool: ...
    # def tell(self) -> int: ...
    # def seek(self, n: int, /) -> object: ...

# Stream shape as required by _EndRecData() and _EndRecData64().
class _SupportsReadSeekTell(Protocol):
    def read(self, n: int = ..., /) -> bytes: ...
    def seek(self, cookie: int, whence: int, /) -> object: ...
    def tell(self) -> int: ...

class _ClosableZipStream(_ZipStream, Protocol):
    def close(self) -> object: ...

class ZipExtFile(io.BufferedIOBase):
    MAX_N: int
    MIN_READ_SIZE: int
    MAX_SEEK_READ: int
    newlines: list[bytes] | None
    mode: _ReadWriteMode
    name: str
    @overload
    def __init__(
        self, fileobj: _ClosableZipStream, mode: _ReadWriteMode, zipinfo: ZipInfo, pwd: bytes | None, close_fileobj: Literal[True]
    ) -> None: ...
    @overload
    def __init__(
        self,
        fileobj: _ClosableZipStream,
        mode: _ReadWriteMode,
        zipinfo: ZipInfo,
        pwd: bytes | None = None,
        *,
        close_fileobj: Literal[True],
    ) -> None: ...
    @overload
    def __init__(
        self,
        fileobj: _ZipStream,
        mode: _ReadWriteMode,
        zipinfo: ZipInfo,
        pwd: bytes | None = None,
        close_fileobj: Literal[False] = False,
    ) -> None: ...
    def read(self, n: int | None = -1) -> bytes: ...
    def readline(self, limit: int = -1) -> bytes: ...  # type: ignore[override]
    def peek(self, n: int = 1) -> bytes: ...
    def read1(self, n: int | None) -> bytes: ...  # type: ignore[override]
    def seek(self, offset: int, whence: int = 0) -> int: ...

class _Writer(Protocol):
    def write(self, s: str, /) -> object: ...

class _ZipReadable(Protocol):
    def seek(self, offset: int, whence: int = 0, /) -> int: ...
    def read(self, n: int = -1, /) -> bytes: ...

class _ZipTellable(Protocol):
    def tell(self) -> int: ...

class _ZipReadableTellable(_ZipReadable, _ZipTellable, Protocol): ...

class _ZipWritable(Protocol):
    def flush(self) -> None: ...
    def close(self) -> None: ...
    def write(self, b: bytes, /) -> int: ...

class ZipFile:
    filename: str | None
    debug: int
    comment: bytes
    filelist: list[ZipInfo]
    fp: IO[bytes] | None
    NameToInfo: dict[str, ZipInfo]
    start_dir: int  # undocumented
    compression: int  # undocumented
    compresslevel: int | None  # undocumented
    mode: _ZipFileMode  # undocumented
    pwd: bytes | None  # undocumented
    # metadata_encoding is new in 3.11
    if sys.version_info >= (3, 11):
        @overload
        def __init__(
            self,
            file: StrPath | IO[bytes],
            mode: _ZipFileMode = "r",
            compression: int = 0,
            allowZip64: bool = True,
            compresslevel: int | None = None,
            *,
            strict_timestamps: bool = True,
            metadata_encoding: str | None = None,
        ) -> None: ...
        # metadata_encoding is only allowed for read mode
        @overload
        def __init__(
            self,
            file: StrPath | _ZipReadable,
            mode: Literal["r"] = "r",
            compression: int = 0,
            allowZip64: bool = True,
            compresslevel: int | None = None,
            *,
            strict_timestamps: bool = True,
            metadata_encoding: str | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self,
            file: StrPath | _ZipWritable,
            mode: Literal["w", "x"] = ...,
            compression: int = 0,
            allowZip64: bool = True,
            compresslevel: int | None = None,
            *,
            strict_timestamps: bool = True,
            metadata_encoding: None = None,
        ) -> None: ...
        @overload
        def __init__(
            self,
            file: StrPath | _ZipReadableTellable,
            mode: Literal["a"] = ...,
            compression: int = 0,
            allowZip64: bool = True,
            compresslevel: int | None = None,
            *,
            strict_timestamps: bool = True,
            metadata_encoding: None = None,
        ) -> None: ...
    else:
        @overload
        def __init__(
            self,
            file: StrPath | IO[bytes],
            mode: _ZipFileMode = "r",
            compression: int = 0,
            allowZip64: bool = True,
            compresslevel: int | None = None,
            *,
            strict_timestamps: bool = True,
        ) -> None: ...
        @overload
        def __init__(
            self,
            file: StrPath | _ZipReadable,
            mode: Literal["r"] = "r",
            compression: int = 0,
            allowZip64: bool = True,
            compresslevel: int | None = None,
            *,
            strict_timestamps: bool = True,
        ) -> None: ...
        @overload
        def __init__(
            self,
            file: StrPath | _ZipWritable,
            mode: Literal["w", "x"] = ...,
            compression: int = 0,
            allowZip64: bool = True,
            compresslevel: int | None = None,
            *,
            strict_timestamps: bool = True,
        ) -> None: ...
        @overload
        def __init__(
            self,
            file: StrPath | _ZipReadableTellable,
            mode: Literal["a"] = ...,
            compression: int = 0,
            allowZip64: bool = True,
            compresslevel: int | None = None,
            *,
            strict_timestamps: bool = True,
        ) -> None: ...

    def __enter__(self) -> Self: ...
    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
    ) -> None: ...
    def close(self) -> None: ...
    def getinfo(self, name: str) -> ZipInfo: ...
    def infolist(self) -> list[ZipInfo]: ...
    def namelist(self) -> list[str]: ...
    def open(
        self, name: str | ZipInfo, mode: _ReadWriteMode = "r", pwd: bytes | None = None, *, force_zip64: bool = False
    ) -> IO[bytes]: ...
    def extract(self, member: str | ZipInfo, path: StrPath | None = None, pwd: bytes | None = None) -> str: ...
    def extractall(
        self, path: StrPath | None = None, members: Iterable[str | ZipInfo] | None = None, pwd: bytes | None = None
    ) -> None: ...
    def printdir(self, file: _Writer | None = None) -> None: ...
    def setpassword(self, pwd: bytes) -> None: ...
    def read(self, name: str | ZipInfo, pwd: bytes | None = None) -> bytes: ...
    def testzip(self) -> str | None: ...
    def write(
        self,
        filename: StrPath,
        arcname: StrPath | None = None,
        compress_type: int | None = None,
        compresslevel: int | None = None,
    ) -> None: ...
    def writestr(
        self,
        zinfo_or_arcname: str | ZipInfo,
        data: SizedBuffer | str,
        compress_type: int | None = None,
        compresslevel: int | None = None,
    ) -> None: ...
    if sys.version_info >= (3, 11):
        def mkdir(self, zinfo_or_directory_name: str | ZipInfo, mode: int = 0o777) -> None: ...
    if sys.version_info >= (3, 14):
        @property
        def data_offset(self) -> int | None: ...

    def __del__(self) -> None: ...

class PyZipFile(ZipFile):
    def __init__(
        self, file: str | IO[bytes], mode: _ZipFileMode = "r", compression: int = 0, allowZip64: bool = True, optimize: int = -1
    ) -> None: ...
    def writepy(self, pathname: str, basename: str = "", filterfunc: Callable[[str], bool] | None = None) -> None: ...

class ZipInfo:
    filename: str
    date_time: _DateTuple
    compress_type: int
    comment: bytes
    extra: bytes
    create_system: int
    create_version: int
    extract_version: int
    reserved: int
    flag_bits: int
    volume: int
    internal_attr: int
    external_attr: int
    header_offset: int
    CRC: int
    compress_size: int
    file_size: int
    orig_filename: str  # undocumented
    if sys.version_info >= (3, 13):
        compress_level: int | None

    def __init__(self, filename: str = "NoName", date_time: _DateTuple = (1980, 1, 1, 0, 0, 0)) -> None: ...
    @classmethod
    def from_file(cls, filename: StrPath, arcname: StrPath | None = None, *, strict_timestamps: bool = True) -> Self: ...
    def is_dir(self) -> bool: ...
    def FileHeader(self, zip64: bool | None = None) -> bytes: ...

if sys.version_info >= (3, 12):
    from zipfile._path import CompleteDirs as CompleteDirs, Path as Path

else:
    class CompleteDirs(ZipFile):
        def resolve_dir(self, name: str) -> str: ...
        @overload
        @classmethod
        def make(cls, source: ZipFile) -> CompleteDirs: ...
        @overload
        @classmethod
        def make(cls, source: StrPath | IO[bytes]) -> Self: ...

    class Path:
        root: CompleteDirs
        at: str
        def __init__(self, root: ZipFile | StrPath | IO[bytes], at: str = "") -> None: ...
        @property
        def name(self) -> str: ...
        @property
        def parent(self) -> PathLike[str]: ...  # undocumented
        if sys.version_info >= (3, 10):
            @property
            def filename(self) -> PathLike[str]: ...  # undocumented
        if sys.version_info >= (3, 11):
            @property
            def suffix(self) -> str: ...
            @property
            def suffixes(self) -> list[str]: ...
            @property
            def stem(self) -> str: ...

        @overload
        def open(
            self,
            mode: Literal["r", "w"] = "r",
            encoding: str | None = None,
            errors: str | None = None,
            newline: str | None = None,
            line_buffering: bool = ...,
            write_through: bool = ...,
            *,
            pwd: bytes | None = None,
        ) -> TextIOWrapper: ...
        @overload
        def open(self, mode: Literal["rb", "wb"], *, pwd: bytes | None = None) -> IO[bytes]: ...

        if sys.version_info >= (3, 10):
            def iterdir(self) -> Iterator[Self]: ...
        else:
            def iterdir(self) -> Iterator[Path]: ...

        def is_dir(self) -> bool: ...
        def is_file(self) -> bool: ...
        def exists(self) -> bool: ...
        def read_text(
            self,
            encoding: str | None = ...,
            errors: str | None = ...,
            newline: str | None = ...,
            line_buffering: bool = ...,
            write_through: bool = ...,
        ) -> str: ...
        def read_bytes(self) -> bytes: ...
        if sys.version_info >= (3, 10):
            def joinpath(self, *other: StrPath) -> Path: ...
        else:
            def joinpath(self, add: StrPath) -> Path: ...  # undocumented

        def __truediv__(self, add: StrPath) -> Path: ...

def is_zipfile(filename: StrOrBytesPath | _SupportsReadSeekTell) -> bool: ...

ZIP64_LIMIT: Final[int]
ZIP_FILECOUNT_LIMIT: Final[int]
ZIP_MAX_COMMENT: Final[int]

ZIP_STORED: Final = 0
ZIP_DEFLATED: Final = 8
ZIP_BZIP2: Final = 12
ZIP_LZMA: Final = 14
if sys.version_info >= (3, 14):
    ZIP_ZSTANDARD: Final = 93

DEFAULT_VERSION: Final[int]
ZIP64_VERSION: Final[int]
BZIP2_VERSION: Final[int]
LZMA_VERSION: Final[int]
if sys.version_info >= (3, 14):
    ZSTANDARD_VERSION: Final[int]
MAX_EXTRACT_VERSION: Final[int]

"""
Read and write ZIP files.

XXX references to utf-8 need further investigation.
"""

import io
import sys
from _typeshed import SizedBuffer, StrOrBytesPath, StrPath
from collections.abc import Callable, Iterable, Iterator
from io import TextIOWrapper
from os import PathLike
from types import TracebackType
from typing import IO, Final, Literal, Protocol, overload, type_check_only
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

class LargeZipFile(Exception):
    """
    Raised when writing a zipfile, the zipfile requires ZIP64 extensions
    and those extensions are disabled.
    """

@type_check_only
class _ZipStream(Protocol):
    def read(self, n: int, /) -> bytes: ...
    # The following methods are optional:
    # def seekable(self) -> bool: ...
    # def tell(self) -> int: ...
    # def seek(self, n: int, /) -> object: ...

# Stream shape as required by _EndRecData() and _EndRecData64().
@type_check_only
class _SupportsReadSeekTell(Protocol):
    def read(self, n: int = ..., /) -> bytes: ...
    def seek(self, cookie: int, whence: int, /) -> object: ...
    def tell(self) -> int: ...

@type_check_only
class _ClosableZipStream(_ZipStream, Protocol):
    def close(self) -> object: ...

class ZipExtFile(io.BufferedIOBase):
    """File-like object for reading an archive member.
    Is returned by ZipFile.open().
    """

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
    def read(self, n: int | None = -1) -> bytes:
        """Read and return up to n bytes.
        If the argument is omitted, None, or negative, data is read and returned until EOF is reached.
        """

    def readline(self, limit: int = -1) -> bytes:  # type: ignore[override]
        """Read and return a line from the stream.

        If limit is specified, at most limit bytes will be read.
        """

    def peek(self, n: int = 1) -> bytes:
        """Returns buffered bytes without advancing the position."""

    def read1(self, n: int | None) -> bytes:  # type: ignore[override]
        """Read up to n bytes with at most one read() system call."""

    def seek(self, offset: int, whence: int = 0) -> int: ...

@type_check_only
class _Writer(Protocol):
    def write(self, s: str, /) -> object: ...

@type_check_only
class _ZipReadable(Protocol):
    def seek(self, offset: int, whence: int = 0, /) -> int: ...
    def read(self, n: int = -1, /) -> bytes: ...

@type_check_only
class _ZipTellable(Protocol):
    def tell(self) -> int: ...

@type_check_only
class _ZipReadableTellable(_ZipReadable, _ZipTellable, Protocol): ...

@type_check_only
class _ZipWritable(Protocol):
    def flush(self) -> None: ...
    def close(self) -> None: ...
    def write(self, b: bytes, /) -> int: ...

class ZipFile:
    """Class with methods to open, read, write, close, list zip files.

    z = ZipFile(file, mode="r", compression=ZIP_STORED, allowZip64=True,
                compresslevel=None)

    file: Either the path to the file, or a file-like object.
          If it is a path, the file will be opened and closed by ZipFile.
    mode: The mode can be either read 'r', write 'w', exclusive create 'x',
          or append 'a'.
    compression: ZIP_STORED (no compression), ZIP_DEFLATED (requires zlib),
                 ZIP_BZIP2 (requires bz2), ZIP_LZMA (requires lzma), or
                 ZIP_ZSTANDARD (requires compression.zstd).
    allowZip64: if True ZipFile will create files with ZIP64 extensions when
                needed, otherwise it will raise an exception when this would
                be necessary.
    compresslevel: None (default for the given compression type) or an integer
                   specifying the level to pass to the compressor.
                   When using ZIP_STORED or ZIP_LZMA this keyword has no effect.
                   When using ZIP_DEFLATED integers 0 through 9 are accepted.
                   When using ZIP_BZIP2 integers 1 through 9 are accepted.
                   When using ZIP_ZSTANDARD integers -7 though 22 are common,
                   see the CompressionParameter enum in compression.zstd for
                   details.

    """

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
        ) -> None:
            """Open the ZIP file with mode read 'r', write 'w', exclusive create 'x',
            or append 'a'.
            """
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
            mode: Literal["w", "x"],
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
            mode: Literal["a"],
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
        ) -> None:
            """Open the ZIP file with mode read 'r', write 'w', exclusive create 'x',
            or append 'a'.
            """

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
            mode: Literal["w", "x"],
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
            mode: Literal["a"],
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
    def close(self) -> None:
        """Close the file, and for mode 'w', 'x' and 'a' write the ending
        records.
        """

    def getinfo(self, name: str) -> ZipInfo:
        """Return the instance of ZipInfo given 'name'."""

    def infolist(self) -> list[ZipInfo]:
        """Return a list of class ZipInfo instances for files in the
        archive.
        """

    def namelist(self) -> list[str]:
        """Return a list of file names in the archive."""

    def open(
        self, name: str | ZipInfo, mode: _ReadWriteMode = "r", pwd: bytes | None = None, *, force_zip64: bool = False
    ) -> IO[bytes]:
        """Return file-like object for 'name'.

        name is a string for the file name within the ZIP file, or a ZipInfo
        object.

        mode should be 'r' to read a file already in the ZIP file, or 'w' to
        write to a file newly added to the archive.

        pwd is the password to decrypt files (only used for reading).

        When writing, if the file size is not known in advance but may exceed
        2 GiB, pass force_zip64 to use the ZIP64 format, which can handle large
        files.  If the size is known in advance, it is best to pass a ZipInfo
        instance for name, with zinfo.file_size set.
        """

    def extract(self, member: str | ZipInfo, path: StrPath | None = None, pwd: bytes | None = None) -> str:
        """Extract a member from the archive to the current working directory,
        using its full name. Its file information is extracted as accurately
        as possible. 'member' may be a filename or a ZipInfo object. You can
        specify a different directory using 'path'. You can specify the
        password to decrypt the file using 'pwd'.
        """

    def extractall(
        self, path: StrPath | None = None, members: Iterable[str | ZipInfo] | None = None, pwd: bytes | None = None
    ) -> None:
        """Extract all members from the archive to the current working
        directory. 'path' specifies a different directory to extract to.
        'members' is optional and must be a subset of the list returned
        by namelist(). You can specify the password to decrypt all files
        using 'pwd'.
        """

    def printdir(self, file: _Writer | None = None) -> None:
        """Print a table of contents for the zip file."""

    def setpassword(self, pwd: bytes) -> None:
        """Set default password for encrypted files."""

    def read(self, name: str | ZipInfo, pwd: bytes | None = None) -> bytes:
        """Return file bytes for name. 'pwd' is the password to decrypt
        encrypted files.
        """

    def testzip(self) -> str | None:
        """Read all the files and check the CRC.

        Return None if all files could be read successfully, or the name
        of the offending file otherwise.
        """

    def write(
        self,
        filename: StrPath,
        arcname: StrPath | None = None,
        compress_type: int | None = None,
        compresslevel: int | None = None,
    ) -> None:
        """Put the bytes from filename into the archive under the name
        arcname.
        """

    def writestr(
        self,
        zinfo_or_arcname: str | ZipInfo,
        data: SizedBuffer | str,
        compress_type: int | None = None,
        compresslevel: int | None = None,
    ) -> None:
        """Write a file into the archive.  The contents is 'data', which
        may be either a 'str' or a 'bytes' instance; if it is a 'str',
        it is encoded as UTF-8 first.
        'zinfo_or_arcname' is either a ZipInfo instance or
        the name of the file in the archive.
        """
    if sys.version_info >= (3, 11):
        def mkdir(self, zinfo_or_directory_name: str | ZipInfo, mode: int = 0o777) -> None:
            """Creates a directory inside the zip archive."""

    def __del__(self) -> None:
        """Call the "close()" method in case the user forgot."""

class PyZipFile(ZipFile):
    """Class to create ZIP archives with Python library files and packages."""

    def __init__(
        self, file: str | IO[bytes], mode: _ZipFileMode = "r", compression: int = 0, allowZip64: bool = True, optimize: int = -1
    ) -> None: ...
    def writepy(self, pathname: str, basename: str = "", filterfunc: Callable[[str], bool] | None = None) -> None:
        """Add all files from "pathname" to the ZIP archive.

        If pathname is a package directory, search the directory and
        all package subdirectories recursively for all *.py and enter
        the modules into the archive.  If pathname is a plain
        directory, listdir *.py and enter all modules.  Else, pathname
        must be a Python *.py file and the module will be put into the
        archive.  Added modules are always module.pyc.
        This method will compile the module.py into module.pyc if
        necessary.
        If filterfunc(pathname) is given, it is called with every argument.
        When it is False, the file or directory is skipped.
        """

class ZipInfo:
    """Class with attributes describing each file in the ZIP archive."""

    __slots__ = (
        "orig_filename",
        "filename",
        "date_time",
        "compress_type",
        "compress_level",
        "comment",
        "extra",
        "create_system",
        "create_version",
        "extract_version",
        "reserved",
        "flag_bits",
        "volume",
        "internal_attr",
        "external_attr",
        "header_offset",
        "CRC",
        "compress_size",
        "file_size",
        "_raw_time",
        "_end_offset",
    )
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
    def from_file(cls, filename: StrPath, arcname: StrPath | None = None, *, strict_timestamps: bool = True) -> Self:
        """Construct an appropriate ZipInfo for a file on the filesystem.

        filename should be the path to a file or directory on the filesystem.

        arcname is the name which it will have within the archive (by default,
        this will be the same as filename, but without a drive letter and with
        leading path separators removed).
        """

    def is_dir(self) -> bool:
        """Return True if this archive member is a directory."""

    def FileHeader(self, zip64: bool | None = None) -> bytes:
        """Return the per-file header as a bytes object.

        When the optional zip64 arg is None rather than a bool, we will
        decide based upon the file_size and compress_size, if known,
        False otherwise.
        """
    if sys.version_info >= (3, 14):
        def _for_archive(self, archive: ZipFile) -> Self:
            """Resolve suitable defaults from the archive.

            Resolve the date_time, compression attributes, and external attributes
            to suitable defaults as used by :method:`ZipFile.writestr`.

            Return self.
            """

if sys.version_info >= (3, 12):
    from zipfile._path import CompleteDirs as CompleteDirs, Path as Path

else:
    class CompleteDirs(ZipFile):
        """
        A ZipFile subclass that ensures that implied directories
        are always included in the namelist.
        """

        def resolve_dir(self, name: str) -> str:
            """
            If the name represents a directory, return that name
            as a directory (with the trailing slash).
            """

        @overload
        @classmethod
        def make(cls, source: ZipFile) -> CompleteDirs:
            """
            Given a source (filename or zipfile), return an
            appropriate CompleteDirs subclass.
            """

        @overload
        @classmethod
        def make(cls, source: StrPath | IO[bytes]) -> Self: ...

    class Path:
        """
        A pathlib-compatible interface for zip files.

        Consider a zip file with this structure::

            .
            ├── a.txt
            └── b
                ├── c.txt
                └── d
                    └── e.txt

        >>> data = io.BytesIO()
        >>> zf = ZipFile(data, 'w')
        >>> zf.writestr('a.txt', 'content of a')
        >>> zf.writestr('b/c.txt', 'content of c')
        >>> zf.writestr('b/d/e.txt', 'content of e')
        >>> zf.filename = 'mem/abcde.zip'

        Path accepts the zipfile object itself or a filename

        >>> root = Path(zf)

        From there, several path operations are available.

        Directory iteration (including the zip file itself):

        >>> a, b = root.iterdir()
        >>> a
        Path('mem/abcde.zip', 'a.txt')
        >>> b
        Path('mem/abcde.zip', 'b/')

        name property:

        >>> b.name
        'b'

        join with divide operator:

        >>> c = b / 'c.txt'
        >>> c
        Path('mem/abcde.zip', 'b/c.txt')
        >>> c.name
        'c.txt'

        Read text:

        >>> c.read_text()
        'content of c'

        existence:

        >>> c.exists()
        True
        >>> (b / 'missing.txt').exists()
        False

        Coercion to string:

        >>> import os
        >>> str(c).replace(os.sep, posixpath.sep)
        'mem/abcde.zip/b/c.txt'

        At the root, ``name``, ``filename``, and ``parent``
        resolve to the zipfile. Note these attributes are not
        valid and will raise a ``ValueError`` if the zipfile
        has no filename.

        >>> root.name
        'abcde.zip'
        >>> str(root.filename).replace(os.sep, posixpath.sep)
        'mem/abcde.zip'
        >>> str(root.parent)
        'mem'
        """

        root: CompleteDirs
        at: str
        def __init__(self, root: ZipFile | StrPath | IO[bytes], at: str = "") -> None:
            """
            Construct a Path from a ZipFile or filename.

            Note: When the source is an existing ZipFile object,
            its type (__class__) will be mutated to a
            specialized type. If the caller wishes to retain the
            original type, the caller should either create a
            separate ZipFile object or pass a filename.
            """

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
        ) -> TextIOWrapper:
            """
            Open this entry as text or binary following the semantics
            of ``pathlib.Path.open()`` by passing arguments through
            to io.TextIOWrapper().
            """

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

def is_zipfile(filename: StrOrBytesPath | _SupportsReadSeekTell) -> bool:
    """Quickly see if a file is a ZIP file by checking the magic number.

    The filename argument may be a file or file-like object too.
    """

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

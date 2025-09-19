"""Temporary files.

This module provides generic, low- and high-level interfaces for
creating temporary files and directories.  All of the interfaces
provided by this module can be used without fear of race conditions
except for 'mktemp'.  'mktemp' is subject to race conditions and
should not be used; it is provided for backward compatibility only.

The default path names are returned as str.  If you supply bytes as
input, all return values will be in bytes.  Ex:

    >>> tempfile.mkstemp()
    (4, '/tmp/tmptpu9nin8')
    >>> tempfile.mkdtemp(suffix=b'')
    b'/tmp/tmppbi8f0hy'

This module also provides some data items to the user:

  TMP_MAX  - maximum number of names that will be tried before
             giving up.
  tempdir  - If this is set to a string before the first use of
             any routine from this module, it will be considered as
             another candidate location to store temporary files.
"""

import io
import sys
from _typeshed import (
    BytesPath,
    GenericPath,
    OpenBinaryMode,
    OpenBinaryModeReading,
    OpenBinaryModeUpdating,
    OpenBinaryModeWriting,
    OpenTextMode,
    ReadableBuffer,
    StrPath,
    WriteableBuffer,
)
from collections.abc import Iterable, Iterator
from types import GenericAlias, TracebackType
from typing import IO, Any, AnyStr, Final, Generic, Literal, overload
from typing_extensions import Self, deprecated

__all__ = [
    "NamedTemporaryFile",
    "TemporaryFile",
    "SpooledTemporaryFile",
    "TemporaryDirectory",
    "mkstemp",
    "mkdtemp",
    "mktemp",
    "TMP_MAX",
    "gettempprefix",
    "tempdir",
    "gettempdir",
    "gettempprefixb",
    "gettempdirb",
]

# global variables
TMP_MAX: Final[int]
tempdir: str | None
template: str

if sys.version_info >= (3, 12):
    @overload
    def NamedTemporaryFile(
        mode: OpenTextMode,
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        delete: bool = True,
        *,
        errors: str | None = None,
        delete_on_close: bool = True,
    ) -> _TemporaryFileWrapper[str]:
        """Create and return a temporary file.
        Arguments:
        'prefix', 'suffix', 'dir' -- as for mkstemp.
        'mode' -- the mode argument to io.open (default "w+b").
        'buffering' -- the buffer size argument to io.open (default -1).
        'encoding' -- the encoding argument to io.open (default None)
        'newline' -- the newline argument to io.open (default None)
        'delete' -- whether the file is automatically deleted (default True).
        'delete_on_close' -- if 'delete', whether the file is deleted on close
           (default True) or otherwise either on context manager exit
           (if context manager was used) or on object finalization. .
        'errors' -- the errors argument to io.open (default None)
        The file is created as mkstemp() would do it.

        Returns an object with a file-like interface; the name of the file
        is accessible as its 'name' attribute.  The file will be automatically
        deleted when it is closed unless the 'delete' argument is set to False.

        On POSIX, NamedTemporaryFiles cannot be automatically deleted if
        the creating process is terminated abruptly with a SIGKILL signal.
        Windows can delete the file even in this case.
        """

    @overload
    def NamedTemporaryFile(
        mode: OpenBinaryMode = "w+b",
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        delete: bool = True,
        *,
        errors: str | None = None,
        delete_on_close: bool = True,
    ) -> _TemporaryFileWrapper[bytes]: ...
    @overload
    def NamedTemporaryFile(
        mode: str = "w+b",
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        delete: bool = True,
        *,
        errors: str | None = None,
        delete_on_close: bool = True,
    ) -> _TemporaryFileWrapper[Any]: ...

else:
    @overload
    def NamedTemporaryFile(
        mode: OpenTextMode,
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        delete: bool = True,
        *,
        errors: str | None = None,
    ) -> _TemporaryFileWrapper[str]:
        """Create and return a temporary file.
        Arguments:
        'prefix', 'suffix', 'dir' -- as for mkstemp.
        'mode' -- the mode argument to io.open (default "w+b").
        'buffering' -- the buffer size argument to io.open (default -1).
        'encoding' -- the encoding argument to io.open (default None)
        'newline' -- the newline argument to io.open (default None)
        'delete' -- whether the file is deleted on close (default True).
        'errors' -- the errors argument to io.open (default None)
        The file is created as mkstemp() would do it.

        Returns an object with a file-like interface; the name of the file
        is accessible as its 'name' attribute.  The file will be automatically
        deleted when it is closed unless the 'delete' argument is set to False.

        On POSIX, NamedTemporaryFiles cannot be automatically deleted if
        the creating process is terminated abruptly with a SIGKILL signal.
        Windows can delete the file even in this case.
        """

    @overload
    def NamedTemporaryFile(
        mode: OpenBinaryMode = "w+b",
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        delete: bool = True,
        *,
        errors: str | None = None,
    ) -> _TemporaryFileWrapper[bytes]: ...
    @overload
    def NamedTemporaryFile(
        mode: str = "w+b",
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        delete: bool = True,
        *,
        errors: str | None = None,
    ) -> _TemporaryFileWrapper[Any]: ...

if sys.platform == "win32":
    TemporaryFile = NamedTemporaryFile
else:
    # See the comments for builtins.open() for an explanation of the overloads.
    @overload
    def TemporaryFile(
        mode: OpenTextMode,
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        *,
        errors: str | None = None,
    ) -> io.TextIOWrapper:
        """Create and return a temporary file.
        Arguments:
        'prefix', 'suffix', 'dir' -- as for mkstemp.
        'mode' -- the mode argument to io.open (default "w+b").
        'buffering' -- the buffer size argument to io.open (default -1).
        'encoding' -- the encoding argument to io.open (default None)
        'newline' -- the newline argument to io.open (default None)
        'errors' -- the errors argument to io.open (default None)
        The file is created as mkstemp() would do it.

        Returns an object with a file-like interface.  The file has no
        name, and will cease to exist when it is closed.
        """

    @overload
    def TemporaryFile(
        mode: OpenBinaryMode,
        buffering: Literal[0],
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        *,
        errors: str | None = None,
    ) -> io.FileIO: ...
    @overload
    def TemporaryFile(
        *,
        buffering: Literal[0],
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        errors: str | None = None,
    ) -> io.FileIO: ...
    @overload
    def TemporaryFile(
        mode: OpenBinaryModeWriting,
        buffering: Literal[-1, 1] = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        *,
        errors: str | None = None,
    ) -> io.BufferedWriter: ...
    @overload
    def TemporaryFile(
        mode: OpenBinaryModeReading,
        buffering: Literal[-1, 1] = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        *,
        errors: str | None = None,
    ) -> io.BufferedReader: ...
    @overload
    def TemporaryFile(
        mode: OpenBinaryModeUpdating = "w+b",
        buffering: Literal[-1, 1] = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        *,
        errors: str | None = None,
    ) -> io.BufferedRandom: ...
    @overload
    def TemporaryFile(
        mode: str = "w+b",
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: AnyStr | None = None,
        prefix: AnyStr | None = None,
        dir: GenericPath[AnyStr] | None = None,
        *,
        errors: str | None = None,
    ) -> IO[Any]: ...

class _TemporaryFileWrapper(IO[AnyStr]):
    """Temporary file wrapper

    This class provides a wrapper around files opened for
    temporary use.  In particular, it seeks to automatically
    remove the file when it is no longer needed.
    """

    file: IO[AnyStr]  # io.TextIOWrapper, io.BufferedReader or io.BufferedWriter
    name: str
    delete: bool
    if sys.version_info >= (3, 12):
        def __init__(self, file: IO[AnyStr], name: str, delete: bool = True, delete_on_close: bool = True) -> None: ...
    else:
        def __init__(self, file: IO[AnyStr], name: str, delete: bool = True) -> None: ...

    def __enter__(self) -> Self: ...
    def __exit__(self, exc: type[BaseException] | None, value: BaseException | None, tb: TracebackType | None) -> None: ...
    def __getattr__(self, name: str) -> Any: ...
    def close(self) -> None:
        """
        Close the temporary file, possibly deleting it.
        """
    # These methods don't exist directly on this object, but
    # are delegated to the underlying IO object through __getattr__.
    # We need to add them here so that this class is concrete.
    def __iter__(self) -> Iterator[AnyStr]: ...
    # FIXME: __next__ doesn't actually exist on this class and should be removed:
    #        see also https://github.com/python/typeshed/pull/5456#discussion_r633068648
    # >>> import tempfile
    # >>> ntf=tempfile.NamedTemporaryFile()
    # >>> next(ntf)
    # Traceback (most recent call last):
    #   File "<stdin>", line 1, in <module>
    # TypeError: '_TemporaryFileWrapper' object is not an iterator
    def __next__(self) -> AnyStr: ...
    def fileno(self) -> int: ...
    def flush(self) -> None: ...
    def isatty(self) -> bool: ...
    def read(self, n: int = ...) -> AnyStr: ...
    def readable(self) -> bool: ...
    def readline(self, limit: int = ...) -> AnyStr: ...
    def readlines(self, hint: int = ...) -> list[AnyStr]: ...
    def seek(self, offset: int, whence: int = ...) -> int: ...
    def seekable(self) -> bool: ...
    def tell(self) -> int: ...
    def truncate(self, size: int | None = ...) -> int: ...
    def writable(self) -> bool: ...
    @overload
    def write(self: _TemporaryFileWrapper[str], s: str, /) -> int: ...
    @overload
    def write(self: _TemporaryFileWrapper[bytes], s: ReadableBuffer, /) -> int: ...
    @overload
    def write(self, s: AnyStr, /) -> int: ...
    @overload
    def writelines(self: _TemporaryFileWrapper[str], lines: Iterable[str]) -> None: ...
    @overload
    def writelines(self: _TemporaryFileWrapper[bytes], lines: Iterable[ReadableBuffer]) -> None: ...
    @overload
    def writelines(self, lines: Iterable[AnyStr]) -> None: ...
    @property
    def closed(self) -> bool: ...

if sys.version_info >= (3, 11):
    _SpooledTemporaryFileBase = io.IOBase
else:
    _SpooledTemporaryFileBase = object

# It does not actually derive from IO[AnyStr], but it does mostly behave
# like one.
class SpooledTemporaryFile(IO[AnyStr], _SpooledTemporaryFileBase):
    """Temporary file wrapper, specialized to switch from BytesIO
    or StringIO to a real file when it exceeds a certain size or
    when a fileno is needed.
    """

    _file: IO[AnyStr]
    @property
    def encoding(self) -> str: ...  # undocumented
    @property
    def newlines(self) -> str | tuple[str, ...] | None: ...  # undocumented
    # bytes needs to go first, as default mode is to open as bytes
    @overload
    def __init__(
        self: SpooledTemporaryFile[bytes],
        max_size: int = 0,
        mode: OpenBinaryMode = "w+b",
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: str | None = None,
        prefix: str | None = None,
        dir: str | None = None,
        *,
        errors: str | None = None,
    ) -> None: ...
    @overload
    def __init__(
        self: SpooledTemporaryFile[str],
        max_size: int,
        mode: OpenTextMode,
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: str | None = None,
        prefix: str | None = None,
        dir: str | None = None,
        *,
        errors: str | None = None,
    ) -> None: ...
    @overload
    def __init__(
        self: SpooledTemporaryFile[str],
        max_size: int = 0,
        *,
        mode: OpenTextMode,
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: str | None = None,
        prefix: str | None = None,
        dir: str | None = None,
        errors: str | None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        max_size: int,
        mode: str,
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: str | None = None,
        prefix: str | None = None,
        dir: str | None = None,
        *,
        errors: str | None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        max_size: int = 0,
        *,
        mode: str,
        buffering: int = -1,
        encoding: str | None = None,
        newline: str | None = None,
        suffix: str | None = None,
        prefix: str | None = None,
        dir: str | None = None,
        errors: str | None = None,
    ) -> None: ...
    @property
    def errors(self) -> str | None: ...
    def rollover(self) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, exc: type[BaseException] | None, value: BaseException | None, tb: TracebackType | None) -> None: ...
    # These methods are copied from the abstract methods of IO, because
    # SpooledTemporaryFile implements IO.
    # See also https://github.com/python/typeshed/pull/2452#issuecomment-420657918.
    def close(self) -> None: ...
    def fileno(self) -> int: ...
    def flush(self) -> None: ...
    def isatty(self) -> bool: ...
    if sys.version_info >= (3, 11):
        # These three work only if the SpooledTemporaryFile is opened in binary mode,
        # because the underlying object in text mode does not have these methods.
        def read1(self, size: int = ..., /) -> AnyStr: ...
        def readinto(self, b: WriteableBuffer) -> int: ...
        def readinto1(self, b: WriteableBuffer) -> int: ...
        def detach(self) -> io.RawIOBase: ...

    def read(self, n: int = ..., /) -> AnyStr: ...
    def readline(self, limit: int | None = ..., /) -> AnyStr: ...  # type: ignore[override]
    def readlines(self, hint: int = ..., /) -> list[AnyStr]: ...  # type: ignore[override]
    def seek(self, offset: int, whence: int = ...) -> int: ...
    def tell(self) -> int: ...
    if sys.version_info >= (3, 11):
        def truncate(self, size: int | None = None) -> int: ...
    else:
        def truncate(self, size: int | None = None) -> None: ...  # type: ignore[override]

    @overload
    def write(self: SpooledTemporaryFile[str], s: str) -> int: ...
    @overload
    def write(self: SpooledTemporaryFile[bytes], s: ReadableBuffer) -> int: ...
    @overload
    def write(self, s: AnyStr) -> int: ...
    @overload  # type: ignore[override]
    def writelines(self: SpooledTemporaryFile[str], iterable: Iterable[str]) -> None: ...
    @overload
    def writelines(self: SpooledTemporaryFile[bytes], iterable: Iterable[ReadableBuffer]) -> None: ...
    @overload
    def writelines(self, iterable: Iterable[AnyStr]) -> None: ...
    def __iter__(self) -> Iterator[AnyStr]: ...  # type: ignore[override]
    # These exist at runtime only on 3.11+.
    def readable(self) -> bool: ...
    def seekable(self) -> bool: ...
    def writable(self) -> bool: ...
    def __next__(self) -> AnyStr:  # type: ignore[override]
        """Implement next(self)."""

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

class TemporaryDirectory(Generic[AnyStr]):
    """Create and return a temporary directory.  This has the same
    behavior as mkdtemp but can be used as a context manager.  For
    example:

        with TemporaryDirectory() as tmpdir:
            ...

    Upon exiting the context, the directory and everything contained
    in it are removed (unless delete=False is passed or an exception
    is raised during cleanup and ignore_cleanup_errors is not True).

    Optional Arguments:
        suffix - A str suffix for the directory name.  (see mkdtemp)
        prefix - A str prefix for the directory name.  (see mkdtemp)
        dir - A directory to create this temp dir in.  (see mkdtemp)
        ignore_cleanup_errors - False; ignore exceptions during cleanup?
        delete - True; whether the directory is automatically deleted.
    """

    name: AnyStr
    if sys.version_info >= (3, 12):
        @overload
        def __init__(
            self: TemporaryDirectory[str],
            suffix: str | None = None,
            prefix: str | None = None,
            dir: StrPath | None = None,
            ignore_cleanup_errors: bool = False,
            *,
            delete: bool = True,
        ) -> None: ...
        @overload
        def __init__(
            self: TemporaryDirectory[bytes],
            suffix: bytes | None = None,
            prefix: bytes | None = None,
            dir: BytesPath | None = None,
            ignore_cleanup_errors: bool = False,
            *,
            delete: bool = True,
        ) -> None: ...
    elif sys.version_info >= (3, 10):
        @overload
        def __init__(
            self: TemporaryDirectory[str],
            suffix: str | None = None,
            prefix: str | None = None,
            dir: StrPath | None = None,
            ignore_cleanup_errors: bool = False,
        ) -> None: ...
        @overload
        def __init__(
            self: TemporaryDirectory[bytes],
            suffix: bytes | None = None,
            prefix: bytes | None = None,
            dir: BytesPath | None = None,
            ignore_cleanup_errors: bool = False,
        ) -> None: ...
    else:
        @overload
        def __init__(
            self: TemporaryDirectory[str], suffix: str | None = None, prefix: str | None = None, dir: StrPath | None = None
        ) -> None: ...
        @overload
        def __init__(
            self: TemporaryDirectory[bytes],
            suffix: bytes | None = None,
            prefix: bytes | None = None,
            dir: BytesPath | None = None,
        ) -> None: ...

    def cleanup(self) -> None: ...
    def __enter__(self) -> AnyStr: ...
    def __exit__(self, exc: type[BaseException] | None, value: BaseException | None, tb: TracebackType | None) -> None: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

# The overloads overlap, but they should still work fine.
@overload
def mkstemp(
    suffix: str | None = None, prefix: str | None = None, dir: StrPath | None = None, text: bool = False
) -> tuple[int, str]:
    """User-callable function to create and return a unique temporary
    file.  The return value is a pair (fd, name) where fd is the
    file descriptor returned by os.open, and name is the filename.

    If 'suffix' is not None, the file name will end with that suffix,
    otherwise there will be no suffix.

    If 'prefix' is not None, the file name will begin with that prefix,
    otherwise a default prefix is used.

    If 'dir' is not None, the file will be created in that directory,
    otherwise a default directory is used.

    If 'text' is specified and true, the file is opened in text
    mode.  Else (the default) the file is opened in binary mode.

    If any of 'suffix', 'prefix' and 'dir' are not None, they must be the
    same type.  If they are bytes, the returned name will be bytes; str
    otherwise.

    The file is readable and writable only by the creating user ID.
    If the operating system uses permission bits to indicate whether a
    file is executable, the file is executable by no one. The file
    descriptor is not inherited by children of this process.

    Caller is responsible for deleting the file when done with it.
    """

@overload
def mkstemp(
    suffix: bytes | None = None, prefix: bytes | None = None, dir: BytesPath | None = None, text: bool = False
) -> tuple[int, bytes]: ...

# The overloads overlap, but they should still work fine.
@overload
def mkdtemp(suffix: str | None = None, prefix: str | None = None, dir: StrPath | None = None) -> str:
    """User-callable function to create and return a unique temporary
    directory.  The return value is the pathname of the directory.

    Arguments are as for mkstemp, except that the 'text' argument is
    not accepted.

    The directory is readable, writable, and searchable only by the
    creating user.

    Caller is responsible for deleting the directory when done with it.
    """

@overload
def mkdtemp(suffix: bytes | None = None, prefix: bytes | None = None, dir: BytesPath | None = None) -> bytes: ...
@deprecated("Deprecated since Python 2.3. Use `mkstemp()` or `NamedTemporaryFile(delete=False)` instead.")
def mktemp(suffix: str = "", prefix: str = "tmp", dir: StrPath | None = None) -> str:
    """User-callable function to return a unique temporary file name.  The
    file is not created.

    Arguments are similar to mkstemp, except that the 'text' argument is
    not accepted, and suffix=None, prefix=None and bytes file names are not
    supported.

    THIS FUNCTION IS UNSAFE AND SHOULD NOT BE USED.  The file name may
    refer to a file that did not exist at some point, but by the time
    you get around to creating it, someone else may have beaten you to
    the punch.
    """

def gettempdirb() -> bytes:
    """Returns tempfile.tempdir as bytes."""

def gettempprefixb() -> bytes:
    """The default prefix for temporary directories as bytes."""

def gettempdir() -> str:
    """Returns tempfile.tempdir as str."""

def gettempprefix() -> str:
    """The default prefix for temporary directories as string."""

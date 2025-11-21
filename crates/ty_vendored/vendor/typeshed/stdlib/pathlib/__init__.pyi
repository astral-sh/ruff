"""Object-oriented filesystem paths.

This module provides classes to represent abstract paths and concrete
paths with operations that have semantics appropriate for different
operating systems.
"""

import sys
import types
from _typeshed import (
    OpenBinaryMode,
    OpenBinaryModeReading,
    OpenBinaryModeUpdating,
    OpenBinaryModeWriting,
    OpenTextMode,
    ReadableBuffer,
    StrOrBytesPath,
    StrPath,
    Unused,
)
from collections.abc import Callable, Generator, Iterator, Sequence
from io import BufferedRandom, BufferedReader, BufferedWriter, FileIO, TextIOWrapper
from os import PathLike, stat_result
from types import GenericAlias, TracebackType
from typing import IO, Any, BinaryIO, ClassVar, Literal, TypeVar, overload
from typing_extensions import Never, Self, deprecated

_PathT = TypeVar("_PathT", bound=PurePath)

__all__ = ["PurePath", "PurePosixPath", "PureWindowsPath", "Path", "PosixPath", "WindowsPath"]

if sys.version_info >= (3, 14):
    from pathlib.types import PathInfo

if sys.version_info >= (3, 13):
    __all__ += ["UnsupportedOperation"]

class PurePath(PathLike[str]):
    """Base class for manipulating paths without I/O.

    PurePath represents a filesystem path and offers operations which
    don't imply any actual filesystem I/O.  Depending on your system,
    instantiating a PurePath will return either a PurePosixPath or a
    PureWindowsPath object.  You can also instantiate either of these classes
    directly, regardless of your system.
    """

    if sys.version_info >= (3, 13):
        __slots__ = (
            "_raw_paths",
            "_drv",
            "_root",
            "_tail_cached",
            "_str",
            "_str_normcase_cached",
            "_parts_normcase_cached",
            "_hash",
        )
    elif sys.version_info >= (3, 12):
        __slots__ = (
            "_raw_paths",
            "_drv",
            "_root",
            "_tail_cached",
            "_str",
            "_str_normcase_cached",
            "_parts_normcase_cached",
            "_lines_cached",
            "_hash",
        )
    else:
        __slots__ = ("_drv", "_root", "_parts", "_str", "_hash", "_pparts", "_cached_cparts")
    if sys.version_info >= (3, 13):
        parser: ClassVar[types.ModuleType]
        def full_match(self, pattern: StrPath, *, case_sensitive: bool | None = None) -> bool:
            """
            Return True if this path matches the given glob-style pattern. The
            pattern is matched against the entire path.
            """

    @property
    def parts(self) -> tuple[str, ...]:
        """An object providing sequence-like access to the
        components in the filesystem path.
        """

    @property
    def drive(self) -> str:
        """The drive prefix (letter or UNC path), if any."""

    @property
    def root(self) -> str:
        """The root of the path, if any."""

    @property
    def anchor(self) -> str:
        """The concatenation of the drive and root, or ''."""

    @property
    def name(self) -> str:
        """The final path component, if any."""

    @property
    def suffix(self) -> str:
        """
        The final component's last suffix, if any.

        This includes the leading period. For example: '.txt'
        """

    @property
    def suffixes(self) -> list[str]:
        """
        A list of the final component's suffixes, if any.

        These include the leading periods. For example: ['.tar', '.gz']
        """

    @property
    def stem(self) -> str:
        """The final path component, minus its last suffix."""
    if sys.version_info >= (3, 12):
        def __new__(cls, *args: StrPath, **kwargs: Unused) -> Self:
            """Construct a PurePath from one or several strings and or existing
            PurePath objects.  The strings and path objects are combined so as
            to yield a canonicalized path, which is incorporated into the
            new PurePath object.
            """

        def __init__(self, *args: StrPath) -> None: ...  # pyright: ignore[reportInconsistentConstructor]
    else:
        def __new__(cls, *args: StrPath) -> Self:
            """Construct a PurePath from one or several strings and or existing
            PurePath objects.  The strings and path objects are combined so as
            to yield a canonicalized path, which is incorporated into the
            new PurePath object.
            """

    def __hash__(self) -> int: ...
    def __fspath__(self) -> str: ...
    def __lt__(self, other: PurePath) -> bool: ...
    def __le__(self, other: PurePath) -> bool: ...
    def __gt__(self, other: PurePath) -> bool: ...
    def __ge__(self, other: PurePath) -> bool: ...
    def __truediv__(self, key: StrPath) -> Self: ...
    def __rtruediv__(self, key: StrPath) -> Self: ...
    def __bytes__(self) -> bytes:
        """Return the bytes representation of the path.  This is only
        recommended to use under Unix.
        """

    def as_posix(self) -> str:
        """Return the string representation of the path with forward (/)
        slashes.
        """

    def as_uri(self) -> str:
        """Return the path as a URI."""

    def is_absolute(self) -> bool:
        """True if the path is absolute (has both a root and, if applicable,
        a drive).
        """
    if sys.version_info >= (3, 13):
        @deprecated(
            "Deprecated since Python 3.13; will be removed in Python 3.15. "
            "Use `os.path.isreserved()` to detect reserved paths on Windows."
        )
        def is_reserved(self) -> bool:
            """Return True if the path contains one of the special names reserved
            by the system, if any.
            """
    else:
        def is_reserved(self) -> bool:
            """Return True if the path contains one of the special names reserved
            by the system, if any.
            """
    if sys.version_info >= (3, 14):
        def is_relative_to(self, other: StrPath) -> bool:
            """Return True if the path is relative to another path or False."""
    elif sys.version_info >= (3, 12):
        def is_relative_to(self, other: StrPath, /, *_deprecated: StrPath) -> bool:
            """Return True if the path is relative to another path or False."""
    else:
        def is_relative_to(self, *other: StrPath) -> bool:
            """Return True if the path is relative to another path or False."""
    if sys.version_info >= (3, 12):
        def match(self, path_pattern: str, *, case_sensitive: bool | None = None) -> bool:
            """
            Return True if this path matches the given pattern. If the pattern is
            relative, matching is done from the right; otherwise, the entire path
            is matched. The recursive wildcard '**' is *not* supported by this
            method.
            """
    else:
        def match(self, path_pattern: str) -> bool:
            """
            Return True if this path matches the given pattern.
            """
    if sys.version_info >= (3, 14):
        def relative_to(self, other: StrPath, *, walk_up: bool = False) -> Self:
            """Return the relative path to another path identified by the passed
            arguments.  If the operation is not possible (because this is not
            related to the other path), raise ValueError.

            The *walk_up* parameter controls whether `..` may be used to resolve
            the path.
            """
    elif sys.version_info >= (3, 12):
        def relative_to(self, other: StrPath, /, *_deprecated: StrPath, walk_up: bool = False) -> Self:
            """Return the relative path to another path identified by the passed
            arguments.  If the operation is not possible (because this is not
            related to the other path), raise ValueError.

            The *walk_up* parameter controls whether `..` may be used to resolve
            the path.
            """
    else:
        def relative_to(self, *other: StrPath) -> Self:
            """Return the relative path to another path identified by the passed
            arguments.  If the operation is not possible (because this is not
            a subpath of the other path), raise ValueError.
            """

    def with_name(self, name: str) -> Self:
        """Return a new path with the file name changed."""

    def with_stem(self, stem: str) -> Self:
        """Return a new path with the stem changed."""

    def with_suffix(self, suffix: str) -> Self:
        """Return a new path with the file suffix changed.  If the path
        has no suffix, add given suffix.  If the given suffix is an empty
        string, remove the suffix from the path.
        """

    def joinpath(self, *other: StrPath) -> Self:
        """Combine this path with one or several arguments, and return a
        new path representing either a subpath (if all arguments are relative
        paths) or a totally different path (if one of the arguments is
        anchored).
        """

    @property
    def parents(self) -> Sequence[Self]:
        """A sequence of this path's logical parents."""

    @property
    def parent(self) -> Self:
        """The logical parent of the path."""
    if sys.version_info < (3, 11):
        def __class_getitem__(cls, type: Any) -> GenericAlias: ...

    if sys.version_info >= (3, 12):
        def with_segments(self, *args: StrPath) -> Self:
            """Construct a new path object from any number of path-like objects.
            Subclasses may override this method to customize how new path objects
            are created from methods like `iterdir()`.
            """

class PurePosixPath(PurePath):
    """PurePath subclass for non-Windows systems.

    On a POSIX system, instantiating a PurePath should return this object.
    However, you can also instantiate it directly on any system.
    """

    __slots__ = ()

class PureWindowsPath(PurePath):
    """PurePath subclass for Windows systems.

    On a Windows system, instantiating a PurePath should return this object.
    However, you can also instantiate it directly on any system.
    """

    __slots__ = ()

class Path(PurePath):
    """PurePath subclass that can make system calls.

    Path represents a filesystem path but unlike PurePath, also offers
    methods to do system calls on path objects. Depending on your system,
    instantiating a Path will return either a PosixPath or a WindowsPath
    object. You can also instantiate a PosixPath or WindowsPath directly,
    but cannot instantiate a WindowsPath on a POSIX system or vice versa.
    """

    if sys.version_info >= (3, 14):
        __slots__ = ("_info",)
    elif sys.version_info >= (3, 10):
        __slots__ = ()
    else:
        __slots__ = ("_accessor",)

    if sys.version_info >= (3, 12):
        def __new__(cls, *args: StrPath, **kwargs: Unused) -> Self: ...  # pyright: ignore[reportInconsistentConstructor]
    else:
        def __new__(cls, *args: StrPath, **kwargs: Unused) -> Self: ...

    @classmethod
    def cwd(cls) -> Self:
        """Return a new path pointing to the current working directory."""
    if sys.version_info >= (3, 10):
        def stat(self, *, follow_symlinks: bool = True) -> stat_result:
            """
            Return the result of the stat() system call on this path, like
            os.stat() does.
            """

        def chmod(self, mode: int, *, follow_symlinks: bool = True) -> None:
            """
            Change the permissions of the path, like os.chmod().
            """
    else:
        def stat(self) -> stat_result:
            """
            Return the result of the stat() system call on this path, like
            os.stat() does.
            """

        def chmod(self, mode: int) -> None:
            """
            Change the permissions of the path, like os.chmod().
            """
    if sys.version_info >= (3, 13):
        @classmethod
        def from_uri(cls, uri: str) -> Self:
            """Return a new path from the given 'file' URI."""

        def is_dir(self, *, follow_symlinks: bool = True) -> bool:
            """
            Whether this path is a directory.
            """

        def is_file(self, *, follow_symlinks: bool = True) -> bool:
            """
            Whether this path is a regular file (also True for symlinks pointing
            to regular files).
            """

        def read_text(self, encoding: str | None = None, errors: str | None = None, newline: str | None = None) -> str:
            """
            Open the file in text mode, read it, and close the file.
            """
    else:
        def __enter__(self) -> Self: ...
        def __exit__(self, t: type[BaseException] | None, v: BaseException | None, tb: TracebackType | None) -> None: ...
        def is_dir(self) -> bool:
            """
            Whether this path is a directory.
            """

        def is_file(self) -> bool:
            """
            Whether this path is a regular file (also True for symlinks pointing
            to regular files).
            """

        def read_text(self, encoding: str | None = None, errors: str | None = None) -> str:
            """
            Open the file in text mode, read it, and close the file.
            """
    if sys.version_info >= (3, 13):
        def glob(self, pattern: str, *, case_sensitive: bool | None = None, recurse_symlinks: bool = False) -> Iterator[Self]:
            """Iterate over this subtree and yield all existing files (of any
            kind, including directories) matching the given relative pattern.
            """

        def rglob(self, pattern: str, *, case_sensitive: bool | None = None, recurse_symlinks: bool = False) -> Iterator[Self]:
            """Recursively yield all existing files (of any kind, including
            directories) matching the given relative pattern, anywhere in
            this subtree.
            """
    elif sys.version_info >= (3, 12):
        def glob(self, pattern: str, *, case_sensitive: bool | None = None) -> Generator[Self, None, None]:
            """Iterate over this subtree and yield all existing files (of any
            kind, including directories) matching the given relative pattern.
            """

        def rglob(self, pattern: str, *, case_sensitive: bool | None = None) -> Generator[Self, None, None]:
            """Recursively yield all existing files (of any kind, including
            directories) matching the given relative pattern, anywhere in
            this subtree.
            """
    else:
        def glob(self, pattern: str) -> Generator[Self, None, None]:
            """Iterate over this subtree and yield all existing files (of any
            kind, including directories) matching the given relative pattern.
            """

        def rglob(self, pattern: str) -> Generator[Self, None, None]:
            """Recursively yield all existing files (of any kind, including
            directories) matching the given relative pattern, anywhere in
            this subtree.
            """
    if sys.version_info >= (3, 12):
        def exists(self, *, follow_symlinks: bool = True) -> bool:
            """
            Whether this path exists.

            This method normally follows symlinks; to check whether a symlink exists,
            add the argument follow_symlinks=False.
            """
    else:
        def exists(self) -> bool:
            """
            Whether this path exists.
            """

    def is_symlink(self) -> bool:
        """
        Whether this path is a symbolic link.
        """

    def is_socket(self) -> bool:
        """
        Whether this path is a socket.
        """

    def is_fifo(self) -> bool:
        """
        Whether this path is a FIFO.
        """

    def is_block_device(self) -> bool:
        """
        Whether this path is a block device.
        """

    def is_char_device(self) -> bool:
        """
        Whether this path is a character device.
        """
    if sys.version_info >= (3, 12):
        def is_junction(self) -> bool:
            """
            Whether this path is a junction.
            """

    def iterdir(self) -> Generator[Self, None, None]:
        """Yield path objects of the directory contents.

        The children are yielded in arbitrary order, and the
        special entries '.' and '..' are not included.
        """

    def lchmod(self, mode: int) -> None:
        """
        Like chmod(), except if the path points to a symlink, the symlink's
        permissions are changed, rather than its target's.
        """

    def lstat(self) -> stat_result:
        """
        Like stat(), except if the path points to a symlink, the symlink's
        status information is returned, rather than its target's.
        """

    def mkdir(self, mode: int = 0o777, parents: bool = False, exist_ok: bool = False) -> None:
        """
        Create a new directory at this given path.
        """
    if sys.version_info >= (3, 14):
        @property
        def info(self) -> PathInfo:
            """
            A PathInfo object that exposes the file type and other file attributes
            of this path.
            """

        @overload
        def move_into(self, target_dir: _PathT) -> _PathT:  # type: ignore[overload-overlap]
            """
            Move this file or directory tree into the given existing directory.
            """

        @overload
        def move_into(self, target_dir: StrPath) -> Self: ...  # type: ignore[overload-overlap]
        @overload
        def move(self, target: _PathT) -> _PathT:  # type: ignore[overload-overlap]
            """
            Recursively move this file or directory tree to the given destination.
            """

        @overload
        def move(self, target: StrPath) -> Self: ...  # type: ignore[overload-overlap]
        @overload
        def copy_into(self, target_dir: _PathT, *, follow_symlinks: bool = True, preserve_metadata: bool = False) -> _PathT:  # type: ignore[overload-overlap]
            """
            Copy this file or directory tree into the given existing directory.
            """

        @overload
        def copy_into(self, target_dir: StrPath, *, follow_symlinks: bool = True, preserve_metadata: bool = False) -> Self: ...  # type: ignore[overload-overlap]
        @overload
        def copy(self, target: _PathT, *, follow_symlinks: bool = True, preserve_metadata: bool = False) -> _PathT:  # type: ignore[overload-overlap]
            """
            Recursively copy this file or directory tree to the given destination.
            """

        @overload
        def copy(self, target: StrPath, *, follow_symlinks: bool = True, preserve_metadata: bool = False) -> Self: ...  # type: ignore[overload-overlap]

    # Adapted from builtins.open
    # Text mode: always returns a TextIOWrapper
    # The Traversable .open in stdlib/importlib/abc.pyi should be kept in sync with this.
    @overload
    def open(
        self,
        mode: OpenTextMode = "r",
        buffering: int = -1,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
    ) -> TextIOWrapper:
        """
        Open the file pointed to by this path and return a file object, as
        the built-in open() function does.
        """
    # Unbuffered binary mode: returns a FileIO
    @overload
    def open(
        self, mode: OpenBinaryMode, buffering: Literal[0], encoding: None = None, errors: None = None, newline: None = None
    ) -> FileIO: ...
    # Buffering is on: return BufferedRandom, BufferedReader, or BufferedWriter
    @overload
    def open(
        self,
        mode: OpenBinaryModeUpdating,
        buffering: Literal[-1, 1] = -1,
        encoding: None = None,
        errors: None = None,
        newline: None = None,
    ) -> BufferedRandom: ...
    @overload
    def open(
        self,
        mode: OpenBinaryModeWriting,
        buffering: Literal[-1, 1] = -1,
        encoding: None = None,
        errors: None = None,
        newline: None = None,
    ) -> BufferedWriter: ...
    @overload
    def open(
        self,
        mode: OpenBinaryModeReading,
        buffering: Literal[-1, 1] = -1,
        encoding: None = None,
        errors: None = None,
        newline: None = None,
    ) -> BufferedReader: ...
    # Buffering cannot be determined: fall back to BinaryIO
    @overload
    def open(
        self, mode: OpenBinaryMode, buffering: int = -1, encoding: None = None, errors: None = None, newline: None = None
    ) -> BinaryIO: ...
    # Fallback if mode is not specified
    @overload
    def open(
        self, mode: str, buffering: int = -1, encoding: str | None = None, errors: str | None = None, newline: str | None = None
    ) -> IO[Any]: ...

    # These methods do "exist" on Windows, but they always raise NotImplementedError.
    if sys.platform == "win32":
        if sys.version_info >= (3, 13):
            # raises UnsupportedOperation:
            def owner(self: Never, *, follow_symlinks: bool = True) -> str:  # type: ignore[misc]
                """
                Return the login name of the file owner.
                """

            def group(self: Never, *, follow_symlinks: bool = True) -> str:  # type: ignore[misc]
                """
                Return the group name of the file gid.
                """
        else:
            def owner(self: Never) -> str:  # type: ignore[misc]
                """
                Return the login name of the file owner.
                """

            def group(self: Never) -> str:  # type: ignore[misc]
                """
                Return the group name of the file gid.
                """
    else:
        if sys.version_info >= (3, 13):
            def owner(self, *, follow_symlinks: bool = True) -> str:
                """
                Return the login name of the file owner.
                """

            def group(self, *, follow_symlinks: bool = True) -> str:
                """
                Return the group name of the file gid.
                """
        else:
            def owner(self) -> str:
                """
                Return the login name of the file owner.
                """

            def group(self) -> str:
                """
                Return the group name of the file gid.
                """
    # This method does "exist" on Windows on <3.12, but always raises NotImplementedError
    # On py312+, it works properly on Windows, as with all other platforms
    if sys.platform == "win32" and sys.version_info < (3, 12):
        def is_mount(self: Never) -> bool:  # type: ignore[misc]
            """
            Check if this path is a POSIX mount point
            """
    else:
        def is_mount(self) -> bool:
            """
            Check if this path is a mount point
            """

    def readlink(self) -> Self:
        """
        Return the path to which the symbolic link points.
        """
    if sys.version_info >= (3, 10):
        def rename(self, target: StrPath) -> Self:
            """
            Rename this path to the target path.

            The target path may be absolute or relative. Relative paths are
            interpreted relative to the current working directory, *not* the
            directory of the Path object.

            Returns the new Path instance pointing to the target path.
            """

        def replace(self, target: StrPath) -> Self:
            """
            Rename this path to the target path, overwriting if that path exists.

            The target path may be absolute or relative. Relative paths are
            interpreted relative to the current working directory, *not* the
            directory of the Path object.

            Returns the new Path instance pointing to the target path.
            """
    else:
        def rename(self, target: str | PurePath) -> Self:
            """
            Rename this path to the target path.

            The target path may be absolute or relative. Relative paths are
            interpreted relative to the current working directory, *not* the
            directory of the Path object.

            Returns the new Path instance pointing to the target path.
            """

        def replace(self, target: str | PurePath) -> Self:
            """
            Rename this path to the target path, overwriting if that path exists.

            The target path may be absolute or relative. Relative paths are
            interpreted relative to the current working directory, *not* the
            directory of the Path object.

            Returns the new Path instance pointing to the target path.
            """

    def resolve(self, strict: bool = False) -> Self:
        """
        Make the path absolute, resolving all symlinks on the way and also
        normalizing it.
        """

    def rmdir(self) -> None:
        """
        Remove this directory.  The directory must be empty.
        """

    def symlink_to(self, target: StrOrBytesPath, target_is_directory: bool = False) -> None:
        """
        Make this path a symlink pointing to the target path.
        Note the order of arguments (link, target) is the reverse of os.symlink.
        """
    if sys.version_info >= (3, 10):
        def hardlink_to(self, target: StrOrBytesPath) -> None:
            """
            Make this path a hard link pointing to the same file as *target*.

            Note the order of arguments (self, target) is the reverse of os.link's.
            """

    def touch(self, mode: int = 0o666, exist_ok: bool = True) -> None:
        """
        Create this file with the given access mode, if it doesn't exist.
        """

    def unlink(self, missing_ok: bool = False) -> None:
        """
        Remove this file or link.
        If the path is a directory, use rmdir() instead.
        """

    @classmethod
    def home(cls) -> Self:
        """Return a new path pointing to expanduser('~')."""

    def absolute(self) -> Self:
        """Return an absolute version of this path
        No normalization or symlink resolution is performed.

        Use resolve() to resolve symlinks and remove '..' segments.
        """

    def expanduser(self) -> Self:
        """Return a new path with expanded ~ and ~user constructs
        (as returned by os.path.expanduser)
        """

    def read_bytes(self) -> bytes:
        """
        Open the file in bytes mode, read it, and close the file.
        """

    def samefile(self, other_path: StrPath) -> bool:
        """Return whether other_path is the same or not as this file
        (as returned by os.path.samefile()).
        """

    def write_bytes(self, data: ReadableBuffer) -> int:
        """
        Open the file in bytes mode, write to it, and close the file.
        """
    if sys.version_info >= (3, 10):
        def write_text(
            self, data: str, encoding: str | None = None, errors: str | None = None, newline: str | None = None
        ) -> int:
            """
            Open the file in text mode, write to it, and close the file.
            """
    else:
        def write_text(self, data: str, encoding: str | None = None, errors: str | None = None) -> int:
            """
            Open the file in text mode, write to it, and close the file.
            """
    if sys.version_info < (3, 12):
        if sys.version_info >= (3, 10):
            @deprecated("Deprecated since Python 3.10; removed in Python 3.12. Use `hardlink_to()` instead.")
            def link_to(self, target: StrOrBytesPath) -> None:
                """
                Make the target path a hard link pointing to this path.

                Note this function does not make this path a hard link to *target*,
                despite the implication of the function and argument names. The order
                of arguments (target, link) is the reverse of Path.symlink_to, but
                matches that of os.link.

                Deprecated since Python 3.10 and scheduled for removal in Python 3.12.
                Use `hardlink_to()` instead.
                """
        else:
            def link_to(self, target: StrOrBytesPath) -> None:
                """
                Make the target path a hard link pointing to this path.

                Note this function does not make this path a hard link to *target*,
                despite the implication of the function and argument names. The order
                of arguments (target, link) is the reverse of Path.symlink_to, but
                matches that of os.link.

                """
    if sys.version_info >= (3, 12):
        def walk(
            self, top_down: bool = True, on_error: Callable[[OSError], object] | None = None, follow_symlinks: bool = False
        ) -> Iterator[tuple[Self, list[str], list[str]]]:
            """Walk the directory tree from this directory, similar to os.walk()."""

class PosixPath(Path, PurePosixPath):
    """Path subclass for non-Windows systems.

    On a POSIX system, instantiating a Path should return this object.
    """

    __slots__ = ()

class WindowsPath(Path, PureWindowsPath):
    """Path subclass for Windows systems.

    On a Windows system, instantiating a Path should return this object.
    """

    __slots__ = ()

if sys.version_info >= (3, 13):
    class UnsupportedOperation(NotImplementedError):
        """An exception that is raised when an unsupported operation is attempted."""

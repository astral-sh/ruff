"""Utility functions for copying and archiving files and directory trees.

XXX The functions here don't copy the resource fork or other metadata on Mac.

"""

import os
import sys
from _typeshed import BytesPath, ExcInfo, FileDescriptorOrPath, MaybeNone, StrOrBytesPath, StrPath, SupportsRead, SupportsWrite
from collections.abc import Callable, Iterable, Sequence
from tarfile import _TarfileFilter
from typing import Any, AnyStr, NamedTuple, NoReturn, Protocol, TypeVar, overload, type_check_only
from typing_extensions import TypeAlias, deprecated

__all__ = [
    "copyfileobj",
    "copyfile",
    "copymode",
    "copystat",
    "copy",
    "copy2",
    "copytree",
    "move",
    "rmtree",
    "Error",
    "SpecialFileError",
    "make_archive",
    "get_archive_formats",
    "register_archive_format",
    "unregister_archive_format",
    "get_unpack_formats",
    "register_unpack_format",
    "unregister_unpack_format",
    "unpack_archive",
    "ignore_patterns",
    "chown",
    "which",
    "get_terminal_size",
    "SameFileError",
    "disk_usage",
]
if sys.version_info < (3, 14):
    __all__ += ["ExecError"]

_StrOrBytesPathT = TypeVar("_StrOrBytesPathT", bound=StrOrBytesPath)
_StrPathT = TypeVar("_StrPathT", bound=StrPath)
_BytesPathT = TypeVar("_BytesPathT", bound=BytesPath)

class Error(OSError): ...

class SameFileError(Error):
    """Raised when source and destination are the same file."""

class SpecialFileError(OSError):
    """Raised when trying to do a kind of operation (e.g. copying) which is
    not supported on a special file (e.g. a named pipe)
    """

if sys.version_info >= (3, 14):
    ExecError = RuntimeError  # Deprecated in Python 3.14; removal scheduled for Python 3.16

else:
    class ExecError(OSError):
        """Raised when a command could not be executed"""

class ReadError(OSError):
    """Raised when an archive cannot be read"""

class RegistryError(Exception):
    """Raised when a registry operation with the archiving
    and unpacking registries fails
    """

def copyfileobj(fsrc: SupportsRead[AnyStr], fdst: SupportsWrite[AnyStr], length: int = 0) -> None:
    """copy data from file-like object fsrc to file-like object fdst"""

def copyfile(src: StrOrBytesPath, dst: _StrOrBytesPathT, *, follow_symlinks: bool = True) -> _StrOrBytesPathT:
    """Copy data from src to dst in the most efficient way possible.

    If follow_symlinks is not set and src is a symbolic link, a new
    symlink will be created instead of copying the file it points to.

    """

def copymode(src: StrOrBytesPath, dst: StrOrBytesPath, *, follow_symlinks: bool = True) -> None:
    """Copy mode bits from src to dst.

    If follow_symlinks is not set, symlinks aren't followed if and only
    if both `src` and `dst` are symlinks.  If `lchmod` isn't available
    (e.g. Linux) this method does nothing.

    """

def copystat(src: StrOrBytesPath, dst: StrOrBytesPath, *, follow_symlinks: bool = True) -> None:
    """Copy file metadata

    Copy the permission bits, last access time, last modification time, and
    flags from `src` to `dst`. On Linux, copystat() also copies the "extended
    attributes" where possible. The file contents, owner, and group are
    unaffected. `src` and `dst` are path-like objects or path names given as
    strings.

    If the optional flag `follow_symlinks` is not set, symlinks aren't
    followed if and only if both `src` and `dst` are symlinks.
    """

@overload
def copy(src: StrPath, dst: _StrPathT, *, follow_symlinks: bool = True) -> _StrPathT | str:
    """Copy data and mode bits ("cp src dst"). Return the file's destination.

    The destination may be a directory.

    If follow_symlinks is false, symlinks won't be followed. This
    resembles GNU's "cp -P src dst".

    If source and destination are the same file, a SameFileError will be
    raised.

    """

@overload
def copy(src: BytesPath, dst: _BytesPathT, *, follow_symlinks: bool = True) -> _BytesPathT | bytes: ...
@overload
def copy2(src: StrPath, dst: _StrPathT, *, follow_symlinks: bool = True) -> _StrPathT | str:
    """Copy data and metadata. Return the file's destination.

    Metadata is copied with copystat(). Please see the copystat function
    for more information.

    The destination may be a directory.

    If follow_symlinks is false, symlinks won't be followed. This
    resembles GNU's "cp -P src dst".
    """

@overload
def copy2(src: BytesPath, dst: _BytesPathT, *, follow_symlinks: bool = True) -> _BytesPathT | bytes: ...
def ignore_patterns(*patterns: StrPath) -> Callable[[Any, list[str]], set[str]]:
    """Function that can be used as copytree() ignore parameter.

    Patterns is a sequence of glob-style patterns
    that are used to exclude files
    """

def copytree(
    src: StrPath,
    dst: _StrPathT,
    symlinks: bool = False,
    ignore: None | Callable[[str, list[str]], Iterable[str]] | Callable[[StrPath, list[str]], Iterable[str]] = None,
    copy_function: Callable[[str, str], object] = ...,
    ignore_dangling_symlinks: bool = False,
    dirs_exist_ok: bool = False,
) -> _StrPathT:
    """Recursively copy a directory tree and return the destination directory.

    If exception(s) occur, an Error is raised with a list of reasons.

    If the optional symlinks flag is true, symbolic links in the
    source tree result in symbolic links in the destination tree; if
    it is false, the contents of the files pointed to by symbolic
    links are copied. If the file pointed to by the symlink doesn't
    exist, an exception will be added in the list of errors raised in
    an Error exception at the end of the copy process.

    You can set the optional ignore_dangling_symlinks flag to true if you
    want to silence this exception. Notice that this has no effect on
    platforms that don't support os.symlink.

    The optional ignore argument is a callable. If given, it
    is called with the `src` parameter, which is the directory
    being visited by copytree(), and `names` which is the list of
    `src` contents, as returned by os.listdir():

        callable(src, names) -> ignored_names

    Since copytree() is called recursively, the callable will be
    called once for each directory that is copied. It returns a
    list of names relative to the `src` directory that should
    not be copied.

    The optional copy_function argument is a callable that will be used
    to copy each file. It will be called with the source path and the
    destination path as arguments. By default, copy2() is used, but any
    function that supports the same signature (like copy()) can be used.

    If dirs_exist_ok is false (the default) and `dst` already exists, a
    `FileExistsError` is raised. If `dirs_exist_ok` is true, the copying
    operation will continue if it encounters existing directories, and files
    within the `dst` tree will be overwritten by corresponding files from the
    `src` tree.
    """

_OnErrorCallback: TypeAlias = Callable[[Callable[..., Any], str, ExcInfo], object]
_OnExcCallback: TypeAlias = Callable[[Callable[..., Any], str, BaseException], object]

@type_check_only
class _RmtreeType(Protocol):
    avoids_symlink_attacks: bool
    if sys.version_info >= (3, 12):
        @overload
        @deprecated("The `onerror` parameter is deprecated. Use `onexc` instead.")
        def __call__(
            self,
            path: StrOrBytesPath,
            ignore_errors: bool,
            onerror: _OnErrorCallback | None,
            *,
            onexc: None = None,
            dir_fd: int | None = None,
        ) -> None: ...
        @overload
        @deprecated("The `onerror` parameter is deprecated. Use `onexc` instead.")
        def __call__(
            self,
            path: StrOrBytesPath,
            ignore_errors: bool = False,
            *,
            onerror: _OnErrorCallback | None,
            onexc: None = None,
            dir_fd: int | None = None,
        ) -> None: ...
        @overload
        def __call__(
            self,
            path: StrOrBytesPath,
            ignore_errors: bool = False,
            *,
            onexc: _OnExcCallback | None = None,
            dir_fd: int | None = None,
        ) -> None: ...
    elif sys.version_info >= (3, 11):
        def __call__(
            self,
            path: StrOrBytesPath,
            ignore_errors: bool = False,
            onerror: _OnErrorCallback | None = None,
            *,
            dir_fd: int | None = None,
        ) -> None: ...

    else:
        def __call__(
            self, path: StrOrBytesPath, ignore_errors: bool = False, onerror: _OnErrorCallback | None = None
        ) -> None: ...

rmtree: _RmtreeType

_CopyFn: TypeAlias = Callable[[str, str], object] | Callable[[StrPath, StrPath], object]

# N.B. shutil.move appears to take bytes arguments, however,
# this does not work when dst is (or is within) an existing directory.
# (#6832)
def move(src: StrPath, dst: _StrPathT, copy_function: _CopyFn = ...) -> _StrPathT | str | MaybeNone:
    """Recursively move a file or directory to another location. This is
    similar to the Unix "mv" command. Return the file or directory's
    destination.

    If dst is an existing directory or a symlink to a directory, then src is
    moved inside that directory. The destination path in that directory must
    not already exist.

    If dst already exists but is not a directory, it may be overwritten
    depending on os.rename() semantics.

    If the destination is on our current filesystem, then rename() is used.
    Otherwise, src is copied to the destination and then removed. Symlinks are
    recreated under the new name if os.rename() fails because of cross
    filesystem renames.

    The optional `copy_function` argument is a callable that will be used
    to copy the source or it will be delegated to `copytree`.
    By default, copy2() is used, but any function that supports the same
    signature (like copy()) can be used.

    A lot more could be done here...  A look at a mv.c shows a lot of
    the issues this implementation glosses over.

    """

class _ntuple_diskusage(NamedTuple):
    """usage(total, used, free)"""

    total: int
    used: int
    free: int

def disk_usage(path: FileDescriptorOrPath) -> _ntuple_diskusage:
    """Return disk usage statistics about the given path.

    Returned value is a named tuple with attributes 'total', 'used' and
    'free', which are the amount of total, used and free space, in bytes.
    """

# While chown can be imported on Windows, it doesn't actually work;
# see https://bugs.python.org/issue33140. We keep it here because it's
# in __all__.
if sys.version_info >= (3, 13):
    @overload
    def chown(
        path: FileDescriptorOrPath,
        user: str | int,
        group: None = None,
        *,
        dir_fd: int | None = None,
        follow_symlinks: bool = True,
    ) -> None:
        """Change owner user and group of the given path.

        user and group can be the uid/gid or the user/group names, and in that case,
        they are converted to their respective uid/gid.

        If dir_fd is set, it should be an open file descriptor to the directory to
        be used as the root of *path* if it is relative.

        If follow_symlinks is set to False and the last element of the path is a
        symbolic link, chown will modify the link itself and not the file being
        referenced by the link.
        """

    @overload
    def chown(
        path: FileDescriptorOrPath,
        user: None = None,
        *,
        group: str | int,
        dir_fd: int | None = None,
        follow_symlinks: bool = True,
    ) -> None: ...
    @overload
    def chown(
        path: FileDescriptorOrPath, user: None, group: str | int, *, dir_fd: int | None = None, follow_symlinks: bool = True
    ) -> None: ...
    @overload
    def chown(
        path: FileDescriptorOrPath, user: str | int, group: str | int, *, dir_fd: int | None = None, follow_symlinks: bool = True
    ) -> None: ...

else:
    @overload
    def chown(path: FileDescriptorOrPath, user: str | int, group: None = None) -> None:
        """Change owner user and group of the given path.

        user and group can be the uid/gid or the user/group names, and in that case,
        they are converted to their respective uid/gid.
        """

    @overload
    def chown(path: FileDescriptorOrPath, user: None = None, *, group: str | int) -> None: ...
    @overload
    def chown(path: FileDescriptorOrPath, user: None, group: str | int) -> None: ...
    @overload
    def chown(path: FileDescriptorOrPath, user: str | int, group: str | int) -> None: ...

if sys.platform == "win32" and sys.version_info < (3, 12):
    @overload
    @deprecated("On Windows before Python 3.12, using a PathLike as `cmd` would always fail or return `None`.")
    def which(cmd: os.PathLike[str], mode: int = 1, path: StrPath | None = None) -> NoReturn:
        """Given a command, mode, and a PATH string, return the path which
        conforms to the given mode on the PATH, or None if there is no such
        file.

        `mode` defaults to os.F_OK | os.X_OK. `path` defaults to the result
        of os.environ.get("PATH"), or can be overridden with a custom search
        path.

        """

@overload
def which(cmd: StrPath, mode: int = 1, path: StrPath | None = None) -> str | None:
    """Given a command, mode, and a PATH string, return the path which
    conforms to the given mode on the PATH, or None if there is no such
    file.

    `mode` defaults to os.F_OK | os.X_OK. `path` defaults to the result
    of os.environ.get("PATH"), or can be overridden with a custom search
    path.

    """

@overload
def which(cmd: bytes, mode: int = 1, path: StrPath | None = None) -> bytes | None: ...
def make_archive(
    base_name: str,
    format: str,
    root_dir: StrPath | None = None,
    base_dir: StrPath | None = None,
    verbose: bool = ...,
    dry_run: bool = ...,
    owner: str | None = None,
    group: str | None = None,
    logger: Any | None = None,
) -> str:
    """Create an archive file (eg. zip or tar).

    'base_name' is the name of the file to create, minus any format-specific
    extension; 'format' is the archive format: one of "zip", "tar", "gztar",
    "bztar", "xztar", or "zstdtar".  Or any other registered format.

    'root_dir' is a directory that will be the root directory of the
    archive; ie. we typically chdir into 'root_dir' before creating the
    archive.  'base_dir' is the directory where we start archiving from;
    ie. 'base_dir' will be the common prefix of all files and
    directories in the archive.  'root_dir' and 'base_dir' both default
    to the current directory.  Returns the name of the archive file.

    'owner' and 'group' are used when creating a tar archive. By default,
    uses the current owner and group.
    """

def get_archive_formats() -> list[tuple[str, str]]:
    """Returns a list of supported formats for archiving and unarchiving.

    Each element of the returned sequence is a tuple (name, description)
    """

@overload
def register_archive_format(
    name: str, function: Callable[..., object], extra_args: Sequence[tuple[str, Any] | list[Any]], description: str = ""
) -> None:
    """Registers an archive format.

    name is the name of the format. function is the callable that will be
    used to create archives. If provided, extra_args is a sequence of
    (name, value) tuples that will be passed as arguments to the callable.
    description can be provided to describe the format, and will be returned
    by the get_archive_formats() function.
    """

@overload
def register_archive_format(
    name: str, function: Callable[[str, str], object], extra_args: None = None, description: str = ""
) -> None: ...
def unregister_archive_format(name: str) -> None: ...
def unpack_archive(
    filename: StrPath, extract_dir: StrPath | None = None, format: str | None = None, *, filter: _TarfileFilter | None = None
) -> None:
    """Unpack an archive.

    `filename` is the name of the archive.

    `extract_dir` is the name of the target directory, where the archive
    is unpacked. If not provided, the current working directory is used.

    `format` is the archive format: one of "zip", "tar", "gztar", "bztar",
    "xztar", or "zstdtar".  Or any other registered format.  If not provided,
    unpack_archive will use the filename extension and see if an unpacker
    was registered for that extension.

    In case none is found, a ValueError is raised.

    If `filter` is given, it is passed to the underlying
    extraction function.
    """

@overload
def register_unpack_format(
    name: str,
    extensions: list[str],
    function: Callable[..., object],
    extra_args: Sequence[tuple[str, Any]],
    description: str = "",
) -> None:
    """Registers an unpack format.

    `name` is the name of the format. `extensions` is a list of extensions
    corresponding to the format.

    `function` is the callable that will be
    used to unpack archives. The callable will receive archives to unpack.
    If it's unable to handle an archive, it needs to raise a ReadError
    exception.

    If provided, `extra_args` is a sequence of
    (name, value) tuples that will be passed as arguments to the callable.
    description can be provided to describe the format, and will be returned
    by the get_unpack_formats() function.
    """

@overload
def register_unpack_format(
    name: str, extensions: list[str], function: Callable[[str, str], object], extra_args: None = None, description: str = ""
) -> None: ...
def unregister_unpack_format(name: str) -> None:
    """Removes the pack format from the registry."""

def get_unpack_formats() -> list[tuple[str, list[str], str]]:
    """Returns a list of supported formats for unpacking.

    Each element of the returned sequence is a tuple
    (name, extensions, description)
    """

def get_terminal_size(fallback: tuple[int, int] = (80, 24)) -> os.terminal_size:
    """Get the size of the terminal window.

    For each of the two dimensions, the environment variable, COLUMNS
    and LINES respectively, is checked. If the variable is defined and
    the value is a positive integer, it is used.

    When COLUMNS or LINES is not defined, which is the common case,
    the terminal connected to sys.__stdout__ is queried
    by invoking os.get_terminal_size.

    If the terminal size cannot be successfully queried, either because
    the system doesn't support querying, or because we are not
    connected to a terminal, the value given in fallback parameter
    is used. Fallback defaults to (80, 24) which is the default
    size used by many terminal emulators.

    The value returned is a named tuple of type os.terminal_size.
    """

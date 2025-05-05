import os
import sys
from _typeshed import BytesPath, ExcInfo, FileDescriptorOrPath, MaybeNone, StrOrBytesPath, StrPath, SupportsRead, SupportsWrite
from collections.abc import Callable, Iterable, Sequence
from tarfile import _TarfileFilter
from typing import Any, AnyStr, NamedTuple, NoReturn, Protocol, TypeVar, overload
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
    "ExecError",
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

_StrOrBytesPathT = TypeVar("_StrOrBytesPathT", bound=StrOrBytesPath)
_StrPathT = TypeVar("_StrPathT", bound=StrPath)
_BytesPathT = TypeVar("_BytesPathT", bound=BytesPath)

class Error(OSError): ...
class SameFileError(Error): ...
class SpecialFileError(OSError): ...
class ExecError(OSError): ...
class ReadError(OSError): ...
class RegistryError(Exception): ...

def copyfileobj(fsrc: SupportsRead[AnyStr], fdst: SupportsWrite[AnyStr], length: int = 0) -> None: ...
def copyfile(src: StrOrBytesPath, dst: _StrOrBytesPathT, *, follow_symlinks: bool = True) -> _StrOrBytesPathT: ...
def copymode(src: StrOrBytesPath, dst: StrOrBytesPath, *, follow_symlinks: bool = True) -> None: ...
def copystat(src: StrOrBytesPath, dst: StrOrBytesPath, *, follow_symlinks: bool = True) -> None: ...
@overload
def copy(src: StrPath, dst: _StrPathT, *, follow_symlinks: bool = True) -> _StrPathT | str: ...
@overload
def copy(src: BytesPath, dst: _BytesPathT, *, follow_symlinks: bool = True) -> _BytesPathT | bytes: ...
@overload
def copy2(src: StrPath, dst: _StrPathT, *, follow_symlinks: bool = True) -> _StrPathT | str: ...
@overload
def copy2(src: BytesPath, dst: _BytesPathT, *, follow_symlinks: bool = True) -> _BytesPathT | bytes: ...
def ignore_patterns(*patterns: StrPath) -> Callable[[Any, list[str]], set[str]]: ...
def copytree(
    src: StrPath,
    dst: _StrPathT,
    symlinks: bool = False,
    ignore: None | Callable[[str, list[str]], Iterable[str]] | Callable[[StrPath, list[str]], Iterable[str]] = None,
    copy_function: Callable[[str, str], object] = ...,
    ignore_dangling_symlinks: bool = False,
    dirs_exist_ok: bool = False,
) -> _StrPathT: ...

_OnErrorCallback: TypeAlias = Callable[[Callable[..., Any], str, ExcInfo], object]
_OnExcCallback: TypeAlias = Callable[[Callable[..., Any], str, BaseException], object]

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
def move(src: StrPath, dst: _StrPathT, copy_function: _CopyFn = ...) -> _StrPathT | str | MaybeNone: ...

class _ntuple_diskusage(NamedTuple):
    total: int
    used: int
    free: int

def disk_usage(path: FileDescriptorOrPath) -> _ntuple_diskusage: ...

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
    ) -> None: ...
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
    def chown(path: FileDescriptorOrPath, user: str | int, group: None = None) -> None: ...
    @overload
    def chown(path: FileDescriptorOrPath, user: None = None, *, group: str | int) -> None: ...
    @overload
    def chown(path: FileDescriptorOrPath, user: None, group: str | int) -> None: ...
    @overload
    def chown(path: FileDescriptorOrPath, user: str | int, group: str | int) -> None: ...

if sys.platform == "win32" and sys.version_info < (3, 12):
    @overload
    @deprecated("On Windows before Python 3.12, using a PathLike as `cmd` would always fail or return `None`.")
    def which(cmd: os.PathLike[str], mode: int = 1, path: StrPath | None = None) -> NoReturn: ...

@overload
def which(cmd: StrPath, mode: int = 1, path: StrPath | None = None) -> str | None: ...
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
) -> str: ...
def get_archive_formats() -> list[tuple[str, str]]: ...
@overload
def register_archive_format(
    name: str, function: Callable[..., object], extra_args: Sequence[tuple[str, Any] | list[Any]], description: str = ""
) -> None: ...
@overload
def register_archive_format(
    name: str, function: Callable[[str, str], object], extra_args: None = None, description: str = ""
) -> None: ...
def unregister_archive_format(name: str) -> None: ...
def unpack_archive(
    filename: StrPath, extract_dir: StrPath | None = None, format: str | None = None, *, filter: _TarfileFilter | None = None
) -> None: ...
@overload
def register_unpack_format(
    name: str,
    extensions: list[str],
    function: Callable[..., object],
    extra_args: Sequence[tuple[str, Any]],
    description: str = "",
) -> None: ...
@overload
def register_unpack_format(
    name: str, extensions: list[str], function: Callable[[str, str], object], extra_args: None = None, description: str = ""
) -> None: ...
def unregister_unpack_format(name: str) -> None: ...
def get_unpack_formats() -> list[tuple[str, list[str], str]]: ...
def get_terminal_size(fallback: tuple[int, int] = (80, 24)) -> os.terminal_size: ...

"""distutils.file_util

Utility functions for operating on single files.
"""

from _typeshed import BytesPath, StrOrBytesPath, StrPath
from collections.abc import Iterable
from typing import Literal, TypeVar, overload

_StrPathT = TypeVar("_StrPathT", bound=StrPath)
_BytesPathT = TypeVar("_BytesPathT", bound=BytesPath)

@overload
def copy_file(
    src: StrPath,
    dst: _StrPathT,
    preserve_mode: bool | Literal[0, 1] = 1,
    preserve_times: bool | Literal[0, 1] = 1,
    update: bool | Literal[0, 1] = 0,
    link: str | None = None,
    verbose: bool | Literal[0, 1] = 1,
    dry_run: bool | Literal[0, 1] = 0,
) -> tuple[_StrPathT | str, bool]:
    """Copy a file 'src' to 'dst'.  If 'dst' is a directory, then 'src' is
    copied there with the same name; otherwise, it must be a filename.  (If
    the file exists, it will be ruthlessly clobbered.)  If 'preserve_mode'
    is true (the default), the file's mode (type and permission bits, or
    whatever is analogous on the current platform) is copied.  If
    'preserve_times' is true (the default), the last-modified and
    last-access times are copied as well.  If 'update' is true, 'src' will
    only be copied if 'dst' does not exist, or if 'dst' does exist but is
    older than 'src'.

    'link' allows you to make hard links (os.link) or symbolic links
    (os.symlink) instead of copying: set it to "hard" or "sym"; if it is
    None (the default), files are copied.  Don't set 'link' on systems that
    don't support it: 'copy_file()' doesn't check if hard or symbolic
    linking is available. If hardlink fails, falls back to
    _copy_file_contents().

    Under Mac OS, uses the native file copy function in macostools; on
    other systems, uses '_copy_file_contents()' to copy file contents.

    Return a tuple (dest_name, copied): 'dest_name' is the actual name of
    the output file, and 'copied' is true if the file was copied (or would
    have been copied, if 'dry_run' true).
    """

@overload
def copy_file(
    src: BytesPath,
    dst: _BytesPathT,
    preserve_mode: bool | Literal[0, 1] = 1,
    preserve_times: bool | Literal[0, 1] = 1,
    update: bool | Literal[0, 1] = 0,
    link: str | None = None,
    verbose: bool | Literal[0, 1] = 1,
    dry_run: bool | Literal[0, 1] = 0,
) -> tuple[_BytesPathT | bytes, bool]: ...
@overload
def move_file(
    src: StrPath, dst: _StrPathT, verbose: bool | Literal[0, 1] = 1, dry_run: bool | Literal[0, 1] = 0
) -> _StrPathT | str:
    """Move a file 'src' to 'dst'.  If 'dst' is a directory, the file will
    be moved into it with the same name; otherwise, 'src' is just renamed
    to 'dst'.  Return the new full name of the file.

    Handles cross-device moves on Unix using 'copy_file()'.  What about
    other systems???
    """

@overload
def move_file(
    src: BytesPath, dst: _BytesPathT, verbose: bool | Literal[0, 1] = 1, dry_run: bool | Literal[0, 1] = 0
) -> _BytesPathT | bytes: ...
def write_file(filename: StrOrBytesPath, contents: Iterable[str]) -> None:
    """Create a file with the specified name and write 'contents' (a
    sequence of strings without line terminators) to it.
    """

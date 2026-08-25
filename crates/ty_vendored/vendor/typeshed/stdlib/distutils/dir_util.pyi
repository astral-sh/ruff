"""distutils.dir_util

Utility functions for manipulating directories and directory trees.
"""

from _typeshed import StrOrBytesPath, StrPath
from collections.abc import Iterable
from typing import Literal

def mkpath(name: str, mode: int = 0o777, verbose: bool | Literal[0, 1] = 1, dry_run: bool | Literal[0, 1] = 0) -> list[str]:
    """Create a directory and any missing ancestor directories.

    If the directory already exists (or if 'name' is the empty string, which
    means the current directory, which of course exists), then do nothing.
    Raise DistutilsFileError if unable to create some directory along the way
    (eg. some sub-path exists, but is a file rather than a directory).
    If 'verbose' is true, print a one-line summary of each mkdir to stdout.
    Return the list of directories actually created.
    """

def create_tree(
    base_dir: StrPath,
    files: Iterable[StrPath],
    mode: int = 0o777,
    verbose: bool | Literal[0, 1] = 1,
    dry_run: bool | Literal[0, 1] = 0,
) -> None:
    """Create all the empty directories under 'base_dir' needed to put 'files'
    there.

    'base_dir' is just the name of a directory which doesn't necessarily
    exist yet; 'files' is a list of filenames to be interpreted relative to
    'base_dir'.  'base_dir' + the directory portion of every file in 'files'
    will be created if it doesn't already exist.  'mode', 'verbose' and
    'dry_run' flags are as for 'mkpath()'.
    """

def copy_tree(
    src: StrPath,
    dst: str,
    preserve_mode: bool | Literal[0, 1] = 1,
    preserve_times: bool | Literal[0, 1] = 1,
    preserve_symlinks: bool | Literal[0, 1] = 0,
    update: bool | Literal[0, 1] = 0,
    verbose: bool | Literal[0, 1] = 1,
    dry_run: bool | Literal[0, 1] = 0,
) -> list[str]:
    """Copy an entire directory tree 'src' to a new location 'dst'.

    Both 'src' and 'dst' must be directory names.  If 'src' is not a
    directory, raise DistutilsFileError.  If 'dst' does not exist, it is
    created with 'mkpath()'.  The end result of the copy is that every
    file in 'src' is copied to 'dst', and directories under 'src' are
    recursively copied to 'dst'.  Return the list of files that were
    copied or might have been copied, using their output name.  The
    return value is unaffected by 'update' or 'dry_run': it is simply
    the list of all files under 'src', with the names changed to be
    under 'dst'.

    'preserve_mode' and 'preserve_times' are the same as for
    'copy_file'; note that they only apply to regular files, not to
    directories.  If 'preserve_symlinks' is true, symlinks will be
    copied as symlinks (on platforms that support them!); otherwise
    (the default), the destination of the symlink will be copied.
    'update' and 'verbose' are the same as for 'copy_file'.
    """

def remove_tree(directory: StrOrBytesPath, verbose: bool | Literal[0, 1] = 1, dry_run: bool | Literal[0, 1] = 0) -> None:
    """Recursively remove an entire directory tree.

    Any errors are ignored (apart from being reported to stdout if 'verbose'
    is true).
    """

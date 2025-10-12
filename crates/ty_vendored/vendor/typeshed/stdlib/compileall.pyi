"""Module/script to byte-compile all .py files to .pyc files.

When called as a script with arguments, this compiles the directories
given as arguments recursively; the -l option prevents it from
recursing into directories.

Without arguments, it compiles all modules on sys.path, without
recursing into subdirectories.  (Even though it should do so for
packages -- for now, you'll have to deal with packages separately.)

See module py_compile for details of the actual byte-compilation.
"""

import sys
from _typeshed import StrPath
from py_compile import PycInvalidationMode
from typing import Any, Protocol, type_check_only

__all__ = ["compile_dir", "compile_file", "compile_path"]

@type_check_only
class _SupportsSearch(Protocol):
    def search(self, string: str, /) -> Any: ...

if sys.version_info >= (3, 10):
    def compile_dir(
        dir: StrPath,
        maxlevels: int | None = None,
        ddir: StrPath | None = None,
        force: bool = False,
        rx: _SupportsSearch | None = None,
        quiet: int = 0,
        legacy: bool = False,
        optimize: int = -1,
        workers: int = 1,
        invalidation_mode: PycInvalidationMode | None = None,
        *,
        stripdir: StrPath | None = None,
        prependdir: StrPath | None = None,
        limit_sl_dest: StrPath | None = None,
        hardlink_dupes: bool = False,
    ) -> bool:
        """Byte-compile all modules in the given directory tree.

        Arguments (only dir is required):

        dir:       the directory to byte-compile
        maxlevels: maximum recursion level (default `sys.getrecursionlimit()`)
        ddir:      the directory that will be prepended to the path to the
                   file as it is compiled into each byte-code file.
        force:     if True, force compilation, even if timestamps are up-to-date
        quiet:     full output with False or 0, errors only with 1,
                   no output with 2
        legacy:    if True, produce legacy pyc paths instead of PEP 3147 paths
        optimize:  int or list of optimization levels or -1 for level of
                   the interpreter. Multiple levels leads to multiple compiled
                   files each with one optimization level.
        workers:   maximum number of parallel workers
        invalidation_mode: how the up-to-dateness of the pyc will be checked
        stripdir:  part of path to left-strip from source file path
        prependdir: path to prepend to beginning of original file path, applied
                   after stripdir
        limit_sl_dest: ignore symlinks if they are pointing outside of
                       the defined path
        hardlink_dupes: hardlink duplicated pyc files
        """

    def compile_file(
        fullname: StrPath,
        ddir: StrPath | None = None,
        force: bool = False,
        rx: _SupportsSearch | None = None,
        quiet: int = 0,
        legacy: bool = False,
        optimize: int = -1,
        invalidation_mode: PycInvalidationMode | None = None,
        *,
        stripdir: StrPath | None = None,
        prependdir: StrPath | None = None,
        limit_sl_dest: StrPath | None = None,
        hardlink_dupes: bool = False,
    ) -> bool:
        """Byte-compile one file.

        Arguments (only fullname is required):

        fullname:  the file to byte-compile
        ddir:      if given, the directory name compiled in to the
                   byte-code file.
        force:     if True, force compilation, even if timestamps are up-to-date
        quiet:     full output with False or 0, errors only with 1,
                   no output with 2
        legacy:    if True, produce legacy pyc paths instead of PEP 3147 paths
        optimize:  int or list of optimization levels or -1 for level of
                   the interpreter. Multiple levels leads to multiple compiled
                   files each with one optimization level.
        invalidation_mode: how the up-to-dateness of the pyc will be checked
        stripdir:  part of path to left-strip from source file path
        prependdir: path to prepend to beginning of original file path, applied
                   after stripdir
        limit_sl_dest: ignore symlinks if they are pointing outside of
                       the defined path.
        hardlink_dupes: hardlink duplicated pyc files
        """

else:
    def compile_dir(
        dir: StrPath,
        maxlevels: int | None = None,
        ddir: StrPath | None = None,
        force: bool = False,
        rx: _SupportsSearch | None = None,
        quiet: int = 0,
        legacy: bool = False,
        optimize: int = -1,
        workers: int = 1,
        invalidation_mode: PycInvalidationMode | None = None,
        *,
        stripdir: str | None = None,  # https://bugs.python.org/issue40447
        prependdir: StrPath | None = None,
        limit_sl_dest: StrPath | None = None,
        hardlink_dupes: bool = False,
    ) -> bool:
        """Byte-compile all modules in the given directory tree.

        Arguments (only dir is required):

        dir:       the directory to byte-compile
        maxlevels: maximum recursion level (default `sys.getrecursionlimit()`)
        ddir:      the directory that will be prepended to the path to the
                   file as it is compiled into each byte-code file.
        force:     if True, force compilation, even if timestamps are up-to-date
        quiet:     full output with False or 0, errors only with 1,
                   no output with 2
        legacy:    if True, produce legacy pyc paths instead of PEP 3147 paths
        optimize:  int or list of optimization levels or -1 for level of
                   the interpreter. Multiple levels leads to multiple compiled
                   files each with one optimization level.
        workers:   maximum number of parallel workers
        invalidation_mode: how the up-to-dateness of the pyc will be checked
        stripdir:  part of path to left-strip from source file path
        prependdir: path to prepend to beginning of original file path, applied
                   after stripdir
        limit_sl_dest: ignore symlinks if they are pointing outside of
                       the defined path
        hardlink_dupes: hardlink duplicated pyc files
        """

    def compile_file(
        fullname: StrPath,
        ddir: StrPath | None = None,
        force: bool = False,
        rx: _SupportsSearch | None = None,
        quiet: int = 0,
        legacy: bool = False,
        optimize: int = -1,
        invalidation_mode: PycInvalidationMode | None = None,
        *,
        stripdir: str | None = None,  # https://bugs.python.org/issue40447
        prependdir: StrPath | None = None,
        limit_sl_dest: StrPath | None = None,
        hardlink_dupes: bool = False,
    ) -> bool:
        """Byte-compile one file.

        Arguments (only fullname is required):

        fullname:  the file to byte-compile
        ddir:      if given, the directory name compiled in to the
                   byte-code file.
        force:     if True, force compilation, even if timestamps are up-to-date
        quiet:     full output with False or 0, errors only with 1,
                   no output with 2
        legacy:    if True, produce legacy pyc paths instead of PEP 3147 paths
        optimize:  int or list of optimization levels or -1 for level of
                   the interpreter. Multiple levels leads to multiple compiled
                   files each with one optimization level.
        invalidation_mode: how the up-to-dateness of the pyc will be checked
        stripdir:  part of path to left-strip from source file path
        prependdir: path to prepend to beginning of original file path, applied
                   after stripdir
        limit_sl_dest: ignore symlinks if they are pointing outside of
                       the defined path.
        hardlink_dupes: hardlink duplicated pyc files
        """

def compile_path(
    skip_curdir: bool = ...,
    maxlevels: int = 0,
    force: bool = False,
    quiet: int = 0,
    legacy: bool = False,
    optimize: int = -1,
    invalidation_mode: PycInvalidationMode | None = None,
) -> bool:
    """Byte-compile all module on sys.path.

    Arguments (all optional):

    skip_curdir: if true, skip current directory (default True)
    maxlevels:   max recursion level (default 0)
    force: as for compile_dir() (default False)
    quiet: as for compile_dir() (default 0)
    legacy: as for compile_dir() (default False)
    optimize: as for compile_dir() (default -1)
    invalidation_mode: as for compiler_dir()
    """

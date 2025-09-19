"""Utilities for comparing files and directories.

Classes:
    dircmp

Functions:
    cmp(f1, f2, shallow=True) -> int
    cmpfiles(a, b, common) -> ([], [], [])
    clear_cache()

"""

import sys
from _typeshed import GenericPath, StrOrBytesPath
from collections.abc import Callable, Iterable, Sequence
from types import GenericAlias
from typing import Any, AnyStr, Final, Generic, Literal

__all__ = ["clear_cache", "cmp", "dircmp", "cmpfiles", "DEFAULT_IGNORES"]

DEFAULT_IGNORES: Final[list[str]]
BUFSIZE: Final = 8192

def cmp(f1: StrOrBytesPath, f2: StrOrBytesPath, shallow: bool | Literal[0, 1] = True) -> bool:
    """Compare two files.

    Arguments:

    f1 -- First file name

    f2 -- Second file name

    shallow -- treat files as identical if their stat signatures (type, size,
               mtime) are identical. Otherwise, files are considered different
               if their sizes or contents differ.  [default: True]

    Return value:

    True if the files are the same, False otherwise.

    This function uses a cache for past comparisons and the results,
    with cache entries invalidated if their stat information
    changes.  The cache may be cleared by calling clear_cache().

    """

def cmpfiles(
    a: GenericPath[AnyStr], b: GenericPath[AnyStr], common: Iterable[GenericPath[AnyStr]], shallow: bool | Literal[0, 1] = True
) -> tuple[list[AnyStr], list[AnyStr], list[AnyStr]]:
    """Compare common files in two directories.

    a, b -- directory names
    common -- list of file names found in both directories
    shallow -- if true, do comparison based solely on stat() information

    Returns a tuple of three lists:
      files that compare equal
      files that are different
      filenames that aren't regular files.

    """

class dircmp(Generic[AnyStr]):
    """A class that manages the comparison of 2 directories.

    dircmp(a, b, ignore=None, hide=None, *, shallow=True)
      A and B are directories.
      IGNORE is a list of names to ignore,
        defaults to DEFAULT_IGNORES.
      HIDE is a list of names to hide,
        defaults to [os.curdir, os.pardir].
      SHALLOW specifies whether to just check the stat signature (do not read
        the files).
        defaults to True.

    High level usage:
      x = dircmp(dir1, dir2)
      x.report() -> prints a report on the differences between dir1 and dir2
       or
      x.report_partial_closure() -> prints report on differences between dir1
            and dir2, and reports on common immediate subdirectories.
      x.report_full_closure() -> like report_partial_closure,
            but fully recursive.

    Attributes:
     left_list, right_list: The files in dir1 and dir2,
        filtered by hide and ignore.
     common: a list of names in both dir1 and dir2.
     left_only, right_only: names only in dir1, dir2.
     common_dirs: subdirectories in both dir1 and dir2.
     common_files: files in both dir1 and dir2.
     common_funny: names in both dir1 and dir2 where the type differs between
        dir1 and dir2, or the name is not stat-able.
     same_files: list of identical files.
     diff_files: list of filenames which differ.
     funny_files: list of files which could not be compared.
     subdirs: a dictionary of dircmp instances (or MyDirCmp instances if this
       object is of type MyDirCmp, a subclass of dircmp), keyed by names
       in common_dirs.
    """

    if sys.version_info >= (3, 13):
        def __init__(
            self,
            a: GenericPath[AnyStr],
            b: GenericPath[AnyStr],
            ignore: Sequence[AnyStr] | None = None,
            hide: Sequence[AnyStr] | None = None,
            *,
            shallow: bool = True,
        ) -> None: ...
    else:
        def __init__(
            self,
            a: GenericPath[AnyStr],
            b: GenericPath[AnyStr],
            ignore: Sequence[AnyStr] | None = None,
            hide: Sequence[AnyStr] | None = None,
        ) -> None: ...
    left: AnyStr
    right: AnyStr
    hide: Sequence[AnyStr]
    ignore: Sequence[AnyStr]
    # These properties are created at runtime by __getattr__
    subdirs: dict[AnyStr, dircmp[AnyStr]]
    same_files: list[AnyStr]
    diff_files: list[AnyStr]
    funny_files: list[AnyStr]
    common_dirs: list[AnyStr]
    common_files: list[AnyStr]
    common_funny: list[AnyStr]
    common: list[AnyStr]
    left_only: list[AnyStr]
    right_only: list[AnyStr]
    left_list: list[AnyStr]
    right_list: list[AnyStr]
    def report(self) -> None: ...
    def report_partial_closure(self) -> None: ...
    def report_full_closure(self) -> None: ...
    methodmap: dict[str, Callable[[], None]]
    def phase0(self) -> None: ...
    def phase1(self) -> None: ...
    def phase2(self) -> None: ...
    def phase3(self) -> None: ...
    def phase4(self) -> None: ...
    def phase4_closure(self) -> None: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

def clear_cache() -> None:
    """Clear the filecmp cache."""

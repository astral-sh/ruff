"""Filename globbing utility."""

import sys
from _typeshed import StrOrBytesPath
from collections.abc import Iterator, Sequence
from typing import AnyStr
from typing_extensions import deprecated

__all__ = ["escape", "glob", "iglob"]

if sys.version_info >= (3, 13):
    __all__ += ["translate"]

if sys.version_info >= (3, 10):
    @deprecated(
        "Deprecated since Python 3.10; will be removed in Python 3.15. Use `glob.glob()` with the *root_dir* argument instead."
    )
    def glob0(dirname: AnyStr, pattern: AnyStr) -> list[AnyStr]: ...
    @deprecated(
        "Deprecated since Python 3.10; will be removed in Python 3.15. Use `glob.glob()` with the *root_dir* argument instead."
    )
    def glob1(dirname: AnyStr, pattern: AnyStr) -> list[AnyStr]: ...

else:
    def glob0(dirname: AnyStr, pattern: AnyStr) -> list[AnyStr]: ...
    def glob1(dirname: AnyStr, pattern: AnyStr) -> list[AnyStr]: ...

if sys.version_info >= (3, 11):
    def glob(
        pathname: AnyStr,
        *,
        root_dir: StrOrBytesPath | None = None,
        dir_fd: int | None = None,
        recursive: bool = False,
        include_hidden: bool = False,
    ) -> list[AnyStr]:
        """Return a list of paths matching a pathname pattern.

        The pattern may contain simple shell-style wildcards a la
        fnmatch. Unlike fnmatch, filenames starting with a
        dot are special cases that are not matched by '*' and '?'
        patterns by default.

        The order of the returned list is undefined. Sort it if you need a
        particular order.

        If `include_hidden` is true, the patterns '*', '?', '**'  will match hidden
        directories.

        If `recursive` is true, the pattern '**' will match any files and
        zero or more directories and subdirectories.
        """

    def iglob(
        pathname: AnyStr,
        *,
        root_dir: StrOrBytesPath | None = None,
        dir_fd: int | None = None,
        recursive: bool = False,
        include_hidden: bool = False,
    ) -> Iterator[AnyStr]:
        """Return an iterator which yields the paths matching a pathname pattern.

        The pattern may contain simple shell-style wildcards a la
        fnmatch. However, unlike fnmatch, filenames starting with a
        dot are special cases that are not matched by '*' and '?'
        patterns.

        The order of the returned paths is undefined. Sort them if you need a
        particular order.

        If recursive is true, the pattern '**' will match any files and
        zero or more directories and subdirectories.
        """

elif sys.version_info >= (3, 10):
    def glob(
        pathname: AnyStr, *, root_dir: StrOrBytesPath | None = None, dir_fd: int | None = None, recursive: bool = False
    ) -> list[AnyStr]:
        """Return a list of paths matching a pathname pattern.

        The pattern may contain simple shell-style wildcards a la
        fnmatch. However, unlike fnmatch, filenames starting with a
        dot are special cases that are not matched by '*' and '?'
        patterns.

        If recursive is true, the pattern '**' will match any files and
        zero or more directories and subdirectories.
        """

    def iglob(
        pathname: AnyStr, *, root_dir: StrOrBytesPath | None = None, dir_fd: int | None = None, recursive: bool = False
    ) -> Iterator[AnyStr]:
        """Return an iterator which yields the paths matching a pathname pattern.

        The pattern may contain simple shell-style wildcards a la
        fnmatch. However, unlike fnmatch, filenames starting with a
        dot are special cases that are not matched by '*' and '?'
        patterns.

        If recursive is true, the pattern '**' will match any files and
        zero or more directories and subdirectories.
        """

else:
    def glob(pathname: AnyStr, *, recursive: bool = False) -> list[AnyStr]:
        """Return a list of paths matching a pathname pattern.

        The pattern may contain simple shell-style wildcards a la
        fnmatch. However, unlike fnmatch, filenames starting with a
        dot are special cases that are not matched by '*' and '?'
        patterns.

        If recursive is true, the pattern '**' will match any files and
        zero or more directories and subdirectories.
        """

    def iglob(pathname: AnyStr, *, recursive: bool = False) -> Iterator[AnyStr]:
        """Return an iterator which yields the paths matching a pathname pattern.

        The pattern may contain simple shell-style wildcards a la
        fnmatch. However, unlike fnmatch, filenames starting with a
        dot are special cases that are not matched by '*' and '?'
        patterns.

        If recursive is true, the pattern '**' will match any files and
        zero or more directories and subdirectories.
        """

def escape(pathname: AnyStr) -> AnyStr:
    """Escape all special characters."""

def has_magic(s: str | bytes) -> bool: ...  # undocumented

if sys.version_info >= (3, 13):
    def translate(pat: str, *, recursive: bool = False, include_hidden: bool = False, seps: Sequence[str] | None = None) -> str:
        """Translate a pathname with shell wildcards to a regular expression.

        If `recursive` is true, the pattern segment '**' will match any number of
        path segments.

        If `include_hidden` is true, wildcards can match path segments beginning
        with a dot ('.').

        If a sequence of separator characters is given to `seps`, they will be
        used to split the pattern into segments and match path separators. If not
        given, os.path.sep and os.path.altsep (where available) are used.
        """

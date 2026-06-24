"""Filename matching with shell patterns.

fnmatch(FILENAME, PATTERN) matches according to the local convention.
fnmatchcase(FILENAME, PATTERN) always takes case in account.

The functions operate by translating the pattern into a regular
expression.  They cache the compiled regular expressions for speed.

The function translate(PATTERN) returns a regular expression
corresponding to PATTERN.  (It does not compile it.)
"""

import sys
from collections.abc import Iterable
from typing import AnyStr

__all__ = ["filter", "fnmatch", "fnmatchcase", "translate"]
if sys.version_info >= (3, 14):
    __all__ += ["filterfalse"]

def fnmatch(name: AnyStr, pat: AnyStr) -> bool:
    """Test whether FILENAME matches PATTERN.

    Patterns are Unix shell style:

    *       matches everything
    ?       matches any single character
    [seq]   matches any character in seq
    [!seq]  matches any char not in seq

    An initial period in FILENAME is not special.
    Both FILENAME and PATTERN are first case-normalized
    if the operating system requires it.
    If you don't want this, use fnmatchcase(FILENAME, PATTERN).
    """

def fnmatchcase(name: AnyStr, pat: AnyStr) -> bool:
    """Test whether FILENAME matches PATTERN, including case.

    This is a version of fnmatch() which doesn't case-normalize
    its arguments.
    """

def filter(names: Iterable[AnyStr], pat: AnyStr) -> list[AnyStr]:
    """Construct a list from those elements of the iterable NAMES that match PAT."""

def translate(pat: str) -> str:
    """Translate a shell PATTERN to a regular expression.

    There is no way to quote meta-characters.
    """

if sys.version_info >= (3, 14):
    def filterfalse(names: Iterable[AnyStr], pat: AnyStr) -> list[AnyStr]:
        """Construct a list from those elements of the iterable NAMES that do not match PAT."""

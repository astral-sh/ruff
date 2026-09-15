"""distutils.dep_util

Utility functions for simple, timestamp-based dependency of files
and groups of files; also, function based entirely on such
timestamp dependency analysis.
"""

from _typeshed import StrOrBytesPath, SupportsLenAndGetItem
from collections.abc import Iterable
from typing import Literal, TypeVar

_SourcesT = TypeVar("_SourcesT", bound=StrOrBytesPath)
_TargetsT = TypeVar("_TargetsT", bound=StrOrBytesPath)

def newer(source: StrOrBytesPath, target: StrOrBytesPath) -> bool | Literal[1]:
    """Return true if 'source' exists and is more recently modified than
    'target', or if 'source' exists and 'target' doesn't.  Return false if
    both exist and 'target' is the same age or younger than 'source'.
    Raise DistutilsFileError if 'source' does not exist.
    """

def newer_pairwise(
    sources: SupportsLenAndGetItem[_SourcesT], targets: SupportsLenAndGetItem[_TargetsT]
) -> tuple[list[_SourcesT], list[_TargetsT]]:
    """Walk two filename lists in parallel, testing if each source is newer
    than its corresponding target.  Return a pair of lists (sources,
    targets) where source is newer than target, according to the semantics
    of 'newer()'.
    """

def newer_group(
    sources: Iterable[StrOrBytesPath], target: StrOrBytesPath, missing: Literal["error", "ignore", "newer"] = "error"
) -> Literal[0, 1]:
    """Return true if 'target' is out-of-date with respect to any file
    listed in 'sources'.  In other words, if 'target' exists and is newer
    than every file in 'sources', return false; otherwise return true.
    'missing' controls what we do when a source file is missing; the
    default ("error") is to blow up with an OSError from inside 'stat()';
    if it is "ignore", we silently drop any missing source files; if it is
    "newer", any missing source files make us assume that 'target' is
    out-of-date (this is handy in "dry-run" mode: it'll make you pretend to
    carry out commands that wouldn't work because inputs are missing, but
    that doesn't matter because you're not actually going to run the
    commands).
    """

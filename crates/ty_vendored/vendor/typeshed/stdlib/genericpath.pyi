"""
Path operations common to more than one OS
Do not use directly.  The OS specific modules import the appropriate
functions from this module themselves.
"""

import os
import sys
from _typeshed import BytesPath, FileDescriptorOrPath, StrOrBytesPath, StrPath, SupportsRichComparisonT
from collections.abc import Sequence
from typing import Literal, NewType, overload
from typing_extensions import LiteralString

__all__ = [
    "commonprefix",
    "exists",
    "getatime",
    "getctime",
    "getmtime",
    "getsize",
    "isdir",
    "isfile",
    "samefile",
    "sameopenfile",
    "samestat",
    "ALLOW_MISSING",
]
if sys.version_info >= (3, 12):
    __all__ += ["islink"]
if sys.version_info >= (3, 13):
    __all__ += ["isjunction", "isdevdrive", "lexists"]

# All overloads can return empty string. Ideally, Literal[""] would be a valid
# Iterable[T], so that list[T] | Literal[""] could be used as a return
# type. But because this only works when T is str, we need Sequence[T] instead.
@overload
def commonprefix(m: Sequence[LiteralString]) -> LiteralString:
    """Given a list of pathnames, returns the longest common leading component"""

@overload
def commonprefix(m: Sequence[StrPath]) -> str: ...
@overload
def commonprefix(m: Sequence[BytesPath]) -> bytes | Literal[""]: ...
@overload
def commonprefix(m: Sequence[list[SupportsRichComparisonT]]) -> Sequence[SupportsRichComparisonT]: ...
@overload
def commonprefix(m: Sequence[tuple[SupportsRichComparisonT, ...]]) -> Sequence[SupportsRichComparisonT]: ...
def exists(path: FileDescriptorOrPath) -> bool:
    """Test whether a path exists.  Returns False for broken symbolic links"""

def getsize(filename: FileDescriptorOrPath) -> int:
    """Return the size of a file, reported by os.stat()."""

def isfile(path: FileDescriptorOrPath) -> bool:
    """Test whether a path is a regular file"""

def isdir(s: FileDescriptorOrPath) -> bool:
    """Return true if the pathname refers to an existing directory."""

if sys.version_info >= (3, 12):
    def islink(path: StrOrBytesPath) -> bool:
        """Test whether a path is a symbolic link"""

# These return float if os.stat_float_times() == True,
# but int is a subclass of float.
def getatime(filename: FileDescriptorOrPath) -> float:
    """Return the last access time of a file, reported by os.stat()."""

def getmtime(filename: FileDescriptorOrPath) -> float:
    """Return the last modification time of a file, reported by os.stat()."""

def getctime(filename: FileDescriptorOrPath) -> float:
    """Return the metadata change time of a file, reported by os.stat()."""

def samefile(f1: FileDescriptorOrPath, f2: FileDescriptorOrPath) -> bool:
    """Test whether two pathnames reference the same actual file or directory

    This is determined by the device number and i-node number and
    raises an exception if an os.stat() call on either pathname fails.
    """

def sameopenfile(fp1: int, fp2: int) -> bool:
    """Test whether two open file objects reference the same file"""

def samestat(s1: os.stat_result, s2: os.stat_result) -> bool:
    """Test whether two stat buffers reference the same file"""

if sys.version_info >= (3, 13):
    def isjunction(path: StrOrBytesPath) -> bool:
        """Test whether a path is a junction
        Junctions are not supported on the current platform
        """

    def isdevdrive(path: StrOrBytesPath) -> bool:
        """Determines whether the specified path is on a Windows Dev Drive.
        Dev Drives are not supported on the current platform
        """

    def lexists(path: StrOrBytesPath) -> bool:
        """Test whether a path exists.  Returns True for broken symbolic links"""

# Added in Python 3.9.23, 3.10.18, 3.11.13, 3.12.11, 3.13.4
_AllowMissingType = NewType("_AllowMissingType", object)
ALLOW_MISSING: _AllowMissingType

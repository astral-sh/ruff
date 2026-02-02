"""
Protocols for supporting classes in pathlib.
"""

from typing import Protocol, runtime_checkable

@runtime_checkable
class PathInfo(Protocol):
    """Protocol for path info objects, which support querying the file type.
    Methods may return cached results.
    """

    def exists(self, *, follow_symlinks: bool = True) -> bool: ...
    def is_dir(self, *, follow_symlinks: bool = True) -> bool: ...
    def is_file(self, *, follow_symlinks: bool = True) -> bool: ...
    def is_symlink(self) -> bool: ...

"""Recognize image file formats based on their first few bytes."""

from _typeshed import StrPath
from collections.abc import Callable
from typing import Any, BinaryIO, Protocol, overload, type_check_only

__all__ = ["what"]

@type_check_only
class _ReadableBinary(Protocol):
    def tell(self) -> int: ...
    def read(self, size: int, /) -> bytes: ...
    def seek(self, offset: int, /) -> Any: ...

@overload
def what(file: StrPath | _ReadableBinary, h: None = None) -> str | None:
    """Return the type of image contained in a file or byte stream."""

@overload
def what(file: Any, h: bytes) -> str | None: ...

tests: list[Callable[[bytes, BinaryIO | None], str | None]]

# _compression is replaced by compression._common._streams on Python 3.14+ (PEP-784)

from _typeshed import Incomplete, WriteableBuffer
from collections.abc import Callable
from io import DEFAULT_BUFFER_SIZE, BufferedIOBase, RawIOBase
from typing import Any, Protocol, type_check_only

BUFFER_SIZE = DEFAULT_BUFFER_SIZE

@type_check_only
class _Reader(Protocol):
    def read(self, n: int, /) -> bytes: ...
    def seekable(self) -> bool: ...
    def seek(self, n: int, /) -> Any: ...

class BaseStream(BufferedIOBase): ...

class DecompressReader(RawIOBase):
    def __init__(
        self,
        fp: _Reader,
        decomp_factory: Callable[..., Incomplete],
        trailing_error: type[Exception] | tuple[type[Exception], ...] = (),
        **decomp_args: Any,  # These are passed to decomp_factory.
    ) -> None: ...
    def readinto(self, b: WriteableBuffer) -> int: ...
    def read(self, size: int = -1) -> bytes: ...
    def seek(self, offset: int, whence: int = 0) -> int: ...

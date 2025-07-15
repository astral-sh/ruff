import sys
from _bz2 import BZ2Compressor as BZ2Compressor, BZ2Decompressor as BZ2Decompressor
from _typeshed import ReadableBuffer, StrOrBytesPath, WriteableBuffer
from collections.abc import Iterable
from io import TextIOWrapper
from typing import IO, Literal, Protocol, SupportsIndex, overload
from typing_extensions import Self, TypeAlias

if sys.version_info >= (3, 14):
    from compression._common._streams import BaseStream, _Reader
else:
    from _compression import BaseStream, _Reader

__all__ = ["BZ2File", "BZ2Compressor", "BZ2Decompressor", "open", "compress", "decompress"]

# The following attributes and methods are optional:
# def fileno(self) -> int: ...
# def close(self) -> object: ...
class _ReadableFileobj(_Reader, Protocol): ...

class _WritableFileobj(Protocol):
    def write(self, b: bytes, /) -> object: ...
    # The following attributes and methods are optional:
    # def fileno(self) -> int: ...
    # def close(self) -> object: ...

def compress(data: ReadableBuffer, compresslevel: int = 9) -> bytes: ...
def decompress(data: ReadableBuffer) -> bytes: ...

_ReadBinaryMode: TypeAlias = Literal["", "r", "rb"]
_WriteBinaryMode: TypeAlias = Literal["w", "wb", "x", "xb", "a", "ab"]
_ReadTextMode: TypeAlias = Literal["rt"]
_WriteTextMode: TypeAlias = Literal["wt", "xt", "at"]

@overload
def open(
    filename: _ReadableFileobj,
    mode: _ReadBinaryMode = "rb",
    compresslevel: int = 9,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> BZ2File: ...
@overload
def open(
    filename: _ReadableFileobj,
    mode: _ReadTextMode,
    compresslevel: int = 9,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: _WritableFileobj,
    mode: _WriteBinaryMode,
    compresslevel: int = 9,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> BZ2File: ...
@overload
def open(
    filename: _WritableFileobj,
    mode: _WriteTextMode,
    compresslevel: int = 9,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: StrOrBytesPath,
    mode: _ReadBinaryMode | _WriteBinaryMode = "rb",
    compresslevel: int = 9,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> BZ2File: ...
@overload
def open(
    filename: StrOrBytesPath,
    mode: _ReadTextMode | _WriteTextMode,
    compresslevel: int = 9,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: StrOrBytesPath | _ReadableFileobj | _WritableFileobj,
    mode: str,
    compresslevel: int = 9,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> BZ2File | TextIOWrapper: ...

class BZ2File(BaseStream, IO[bytes]):
    def __enter__(self) -> Self: ...
    @overload
    def __init__(self, filename: _WritableFileobj, mode: _WriteBinaryMode, *, compresslevel: int = 9) -> None: ...
    @overload
    def __init__(self, filename: _ReadableFileobj, mode: _ReadBinaryMode = "r", *, compresslevel: int = 9) -> None: ...
    @overload
    def __init__(
        self, filename: StrOrBytesPath, mode: _ReadBinaryMode | _WriteBinaryMode = "r", *, compresslevel: int = 9
    ) -> None: ...
    def read(self, size: int | None = -1) -> bytes: ...
    def read1(self, size: int = -1) -> bytes: ...
    def readline(self, size: SupportsIndex = -1) -> bytes: ...  # type: ignore[override]
    def readinto(self, b: WriteableBuffer) -> int: ...
    def readlines(self, size: SupportsIndex = -1) -> list[bytes]: ...
    def peek(self, n: int = 0) -> bytes: ...
    def seek(self, offset: int, whence: int = 0) -> int: ...
    def write(self, data: ReadableBuffer) -> int: ...
    def writelines(self, seq: Iterable[ReadableBuffer]) -> None: ...

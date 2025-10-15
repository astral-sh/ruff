import sys
from _lzma import (
    CHECK_CRC32 as CHECK_CRC32,
    CHECK_CRC64 as CHECK_CRC64,
    CHECK_ID_MAX as CHECK_ID_MAX,
    CHECK_NONE as CHECK_NONE,
    CHECK_SHA256 as CHECK_SHA256,
    CHECK_UNKNOWN as CHECK_UNKNOWN,
    FILTER_ARM as FILTER_ARM,
    FILTER_ARMTHUMB as FILTER_ARMTHUMB,
    FILTER_DELTA as FILTER_DELTA,
    FILTER_IA64 as FILTER_IA64,
    FILTER_LZMA1 as FILTER_LZMA1,
    FILTER_LZMA2 as FILTER_LZMA2,
    FILTER_POWERPC as FILTER_POWERPC,
    FILTER_SPARC as FILTER_SPARC,
    FILTER_X86 as FILTER_X86,
    FORMAT_ALONE as FORMAT_ALONE,
    FORMAT_AUTO as FORMAT_AUTO,
    FORMAT_RAW as FORMAT_RAW,
    FORMAT_XZ as FORMAT_XZ,
    MF_BT2 as MF_BT2,
    MF_BT3 as MF_BT3,
    MF_BT4 as MF_BT4,
    MF_HC3 as MF_HC3,
    MF_HC4 as MF_HC4,
    MODE_FAST as MODE_FAST,
    MODE_NORMAL as MODE_NORMAL,
    PRESET_DEFAULT as PRESET_DEFAULT,
    PRESET_EXTREME as PRESET_EXTREME,
    LZMACompressor as LZMACompressor,
    LZMADecompressor as LZMADecompressor,
    LZMAError as LZMAError,
    _FilterChain,
    is_check_supported as is_check_supported,
)
from _typeshed import ReadableBuffer, StrOrBytesPath
from io import TextIOWrapper
from typing import IO, Literal, overload
from typing_extensions import Self, TypeAlias

if sys.version_info >= (3, 14):
    from compression._common._streams import BaseStream
else:
    from _compression import BaseStream

__all__ = [
    "CHECK_NONE",
    "CHECK_CRC32",
    "CHECK_CRC64",
    "CHECK_SHA256",
    "CHECK_ID_MAX",
    "CHECK_UNKNOWN",
    "FILTER_LZMA1",
    "FILTER_LZMA2",
    "FILTER_DELTA",
    "FILTER_X86",
    "FILTER_IA64",
    "FILTER_ARM",
    "FILTER_ARMTHUMB",
    "FILTER_POWERPC",
    "FILTER_SPARC",
    "FORMAT_AUTO",
    "FORMAT_XZ",
    "FORMAT_ALONE",
    "FORMAT_RAW",
    "MF_HC3",
    "MF_HC4",
    "MF_BT2",
    "MF_BT3",
    "MF_BT4",
    "MODE_FAST",
    "MODE_NORMAL",
    "PRESET_DEFAULT",
    "PRESET_EXTREME",
    "LZMACompressor",
    "LZMADecompressor",
    "LZMAFile",
    "LZMAError",
    "open",
    "compress",
    "decompress",
    "is_check_supported",
]

_OpenBinaryWritingMode: TypeAlias = Literal["w", "wb", "x", "xb", "a", "ab"]
_OpenTextWritingMode: TypeAlias = Literal["wt", "xt", "at"]

_PathOrFile: TypeAlias = StrOrBytesPath | IO[bytes]

class LZMAFile(BaseStream, IO[bytes]):  # type: ignore[misc]  # incompatible definitions of writelines in the base classes
    def __init__(
        self,
        filename: _PathOrFile | None = None,
        mode: str = "r",
        *,
        format: int | None = None,
        check: int = -1,
        preset: int | None = None,
        filters: _FilterChain | None = None,
    ) -> None: ...
    def __enter__(self) -> Self: ...
    def peek(self, size: int = -1) -> bytes: ...
    def read(self, size: int | None = -1) -> bytes: ...
    def read1(self, size: int = -1) -> bytes: ...
    def readline(self, size: int | None = -1) -> bytes: ...
    def write(self, data: ReadableBuffer) -> int: ...
    def seek(self, offset: int, whence: int = 0) -> int: ...

@overload
def open(
    filename: _PathOrFile,
    mode: Literal["r", "rb"] = "rb",
    *,
    format: int | None = None,
    check: Literal[-1] = -1,
    preset: None = None,
    filters: _FilterChain | None = None,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> LZMAFile: ...
@overload
def open(
    filename: _PathOrFile,
    mode: _OpenBinaryWritingMode,
    *,
    format: int | None = None,
    check: int = -1,
    preset: int | None = None,
    filters: _FilterChain | None = None,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
) -> LZMAFile: ...
@overload
def open(
    filename: StrOrBytesPath,
    mode: Literal["rt"],
    *,
    format: int | None = None,
    check: Literal[-1] = -1,
    preset: None = None,
    filters: _FilterChain | None = None,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: StrOrBytesPath,
    mode: _OpenTextWritingMode,
    *,
    format: int | None = None,
    check: int = -1,
    preset: int | None = None,
    filters: _FilterChain | None = None,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> TextIOWrapper: ...
@overload
def open(
    filename: _PathOrFile,
    mode: str,
    *,
    format: int | None = None,
    check: int = -1,
    preset: int | None = None,
    filters: _FilterChain | None = None,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
) -> LZMAFile | TextIOWrapper: ...
def compress(
    data: ReadableBuffer, format: int = 1, check: int = -1, preset: int | None = None, filters: _FilterChain | None = None
) -> bytes: ...
def decompress(
    data: ReadableBuffer, format: int = 0, memlimit: int | None = None, filters: _FilterChain | None = None
) -> bytes: ...

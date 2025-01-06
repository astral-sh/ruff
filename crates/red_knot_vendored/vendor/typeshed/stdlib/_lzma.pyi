import sys
from _typeshed import ReadableBuffer
from collections.abc import Mapping, Sequence
from typing import Any, Final, final
from typing_extensions import Self, TypeAlias

_FilterChain: TypeAlias = Sequence[Mapping[str, Any]]

FORMAT_AUTO: Final = 0
FORMAT_XZ: Final = 1
FORMAT_ALONE: Final = 2
FORMAT_RAW: Final = 3
CHECK_NONE: Final = 0
CHECK_CRC32: Final = 1
CHECK_CRC64: Final = 4
CHECK_SHA256: Final = 10
CHECK_ID_MAX: Final = 15
CHECK_UNKNOWN: Final = 16
FILTER_LZMA1: int  # v big number
FILTER_LZMA2: Final = 33
FILTER_DELTA: Final = 3
FILTER_X86: Final = 4
FILTER_IA64: Final = 6
FILTER_ARM: Final = 7
FILTER_ARMTHUMB: Final = 8
FILTER_SPARC: Final = 9
FILTER_POWERPC: Final = 5
MF_HC3: Final = 3
MF_HC4: Final = 4
MF_BT2: Final = 18
MF_BT3: Final = 19
MF_BT4: Final = 20
MODE_FAST: Final = 1
MODE_NORMAL: Final = 2
PRESET_DEFAULT: Final = 6
PRESET_EXTREME: int  # v big number

@final
class LZMADecompressor:
    if sys.version_info >= (3, 12):
        def __new__(cls, format: int | None = ..., memlimit: int | None = ..., filters: _FilterChain | None = ...) -> Self: ...
    else:
        def __init__(self, format: int | None = ..., memlimit: int | None = ..., filters: _FilterChain | None = ...) -> None: ...

    def decompress(self, data: ReadableBuffer, max_length: int = -1) -> bytes: ...
    @property
    def check(self) -> int: ...
    @property
    def eof(self) -> bool: ...
    @property
    def unused_data(self) -> bytes: ...
    @property
    def needs_input(self) -> bool: ...

@final
class LZMACompressor:
    if sys.version_info >= (3, 12):
        def __new__(
            cls, format: int | None = ..., check: int = ..., preset: int | None = ..., filters: _FilterChain | None = ...
        ) -> Self: ...
    else:
        def __init__(
            self, format: int | None = ..., check: int = ..., preset: int | None = ..., filters: _FilterChain | None = ...
        ) -> None: ...

    def compress(self, data: ReadableBuffer, /) -> bytes: ...
    def flush(self) -> bytes: ...

class LZMAError(Exception): ...

def is_check_supported(check_id: int, /) -> bool: ...

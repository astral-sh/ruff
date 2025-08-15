import sys
from _typeshed import ReadableBuffer
from typing import Any, Final, final, type_check_only
from typing_extensions import Self

DEFLATED: Final = 8
DEF_MEM_LEVEL: Final[int]
DEF_BUF_SIZE: Final = 16384
MAX_WBITS: Final[int]
ZLIB_VERSION: Final[str]
ZLIB_RUNTIME_VERSION: Final[str]
Z_NO_COMPRESSION: Final = 0
Z_PARTIAL_FLUSH: Final = 1
Z_BEST_COMPRESSION: Final = 9
Z_BEST_SPEED: Final = 1
Z_BLOCK: Final = 5
Z_DEFAULT_COMPRESSION: Final = -1
Z_DEFAULT_STRATEGY: Final = 0
Z_FILTERED: Final = 1
Z_FINISH: Final = 4
Z_FIXED: Final = 4
Z_FULL_FLUSH: Final = 3
Z_HUFFMAN_ONLY: Final = 2
Z_NO_FLUSH: Final = 0
Z_RLE: Final = 3
Z_SYNC_FLUSH: Final = 2
Z_TREES: Final = 6

if sys.version_info >= (3, 14) and sys.platform == "win32":
    # Available when zlib was built with zlib-ng, usually only on Windows
    ZLIBNG_VERSION: Final[str]

class error(Exception): ...

# This class is not exposed at runtime. It calls itself zlib.Compress.
@final
@type_check_only
class _Compress:
    def __copy__(self) -> Self: ...
    def __deepcopy__(self, memo: Any, /) -> Self: ...
    def compress(self, data: ReadableBuffer, /) -> bytes: ...
    def flush(self, mode: int = 4, /) -> bytes: ...
    def copy(self) -> _Compress: ...

# This class is not exposed at runtime. It calls itself zlib.Decompress.
@final
@type_check_only
class _Decompress:
    @property
    def unused_data(self) -> bytes: ...
    @property
    def unconsumed_tail(self) -> bytes: ...
    @property
    def eof(self) -> bool: ...
    def __copy__(self) -> Self: ...
    def __deepcopy__(self, memo: Any, /) -> Self: ...
    def decompress(self, data: ReadableBuffer, /, max_length: int = 0) -> bytes: ...
    def flush(self, length: int = 16384, /) -> bytes: ...
    def copy(self) -> _Decompress: ...

def adler32(data: ReadableBuffer, value: int = 1, /) -> int: ...

if sys.version_info >= (3, 11):
    def compress(data: ReadableBuffer, /, level: int = -1, wbits: int = 15) -> bytes: ...

else:
    def compress(data: ReadableBuffer, /, level: int = -1) -> bytes: ...

def compressobj(
    level: int = -1, method: int = 8, wbits: int = 15, memLevel: int = 8, strategy: int = 0, zdict: ReadableBuffer | None = None
) -> _Compress: ...
def crc32(data: ReadableBuffer, value: int = 0, /) -> int: ...
def decompress(data: ReadableBuffer, /, wbits: int = 15, bufsize: int = 16384) -> bytes: ...
def decompressobj(wbits: int = 15, zdict: ReadableBuffer = b"") -> _Decompress: ...

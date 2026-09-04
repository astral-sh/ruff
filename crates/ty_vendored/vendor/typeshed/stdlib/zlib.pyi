"""The functions in this module allow compression and decompression using the
zlib library, which is based on GNU zip.

adler32(string[, start]) -- Compute an Adler-32 checksum.
adler32_combine(adler1, adler2, len2, /) -- Combine two Adler-32 checksums.
compress(data[, level]) -- Compress data, with compression level 0-9 or -1.
compressobj([level[, ...]]) -- Return a compressor object.
crc32(string[, start]) -- Compute a CRC-32 checksum.
crc32_combine(crc1, crc2, len2, /) -- Combine two CRC-32 checksums.
decompress(string,[wbits],[bufsize]) -- Decompresses a compressed string.
decompressobj([wbits[, zdict]]) -- Return a decompressor object.

'wbits' is window buffer size and container format.
Compressor objects support compress() and flush() methods; decompressor
objects support decompress() and flush().
"""
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

if sys.version_info >= (3, 14):
    # Available when zlib was built with zlib-ng
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

def adler32(data: ReadableBuffer, value: int = 1, /) -> int:
    """Compute an Adler-32 checksum of data.

  value
    Starting value of the checksum.

The returned checksum is an integer.
"""

if sys.version_info >= (3, 15):
    def adler32_combine(adler1: int, adler2: int, len2: int, /) -> int:
        """Combine two Adler-32 checksums into one.

  adler1
    Adler-32 checksum for sequence A
  adler2
    Adler-32 checksum for sequence B
  len2
    Length of sequence B

Given the Adler-32 checksum 'adler1' of a sequence A and the
Adler-32 checksum 'adler2' of a sequence B of length 'len2',
return the Adler-32 checksum of A and B concatenated.
"""

if sys.version_info >= (3, 11):
    def compress(data: ReadableBuffer, /, level: int = -1, wbits: int = 15) -> bytes:
        """Returns a bytes object containing compressed data.

  data
    Binary data to be compressed.
  level
    Compression level, in 0-9 or -1.
  wbits
    The window buffer size and container format.
"""

else:
    def compress(data: ReadableBuffer, /, level: int = -1) -> bytes:
        """Returns a bytes object containing compressed data.

  data
    Binary data to be compressed.
  level
    Compression level, in 0-9 or -1.
"""

def compressobj(
    level: int = -1, method: int = 8, wbits: int = 15, memLevel: int = 8, strategy: int = 0, zdict: ReadableBuffer | None = None
) -> _Compress:
    """Return a compressor object.

  level
    The compression level (an integer in the range 0-9 or -1; default is
    currently equivalent to 6).  Higher compression levels are slower,
    but produce smaller results.
  method
    The compression algorithm.  If given, this must be DEFLATED.
  wbits
    +9 to +15: The base-two logarithm of the window size.  Include a zlib
        container.
    -9 to -15: Generate a raw stream.
    +25 to +31: Include a gzip container.
  memLevel
    Controls the amount of memory used for internal compression state.
    Valid values range from 1 to 9.  Higher values result in higher memory
    usage, faster compression, and smaller output.
  strategy
    Used to tune the compression algorithm.  Possible values are
    Z_DEFAULT_STRATEGY, Z_FILTERED, and Z_HUFFMAN_ONLY.
  zdict
    The predefined compression dictionary - a sequence of bytes
    containing subsequences that are likely to occur in the input data.
"""
def crc32(data: ReadableBuffer, value: int = 0, /) -> int:
    """Compute a CRC-32 checksum of data.

  value
    Starting value of the checksum.

The returned checksum is an integer.
"""

if sys.version_info >= (3, 15):
    def crc32_combine(crc1: int, crc2: int, len2: int, /) -> int:
        """Combine two CRC-32 checksums into one.

  crc1
    CRC-32 checksum for sequence A
  crc2
    CRC-32 checksum for sequence B
  len2
    Length of sequence B

Given the CRC-32 checksum 'crc1' of a sequence A and the
CRC-32 checksum 'crc2' of a sequence B of length 'len2',
return the CRC-32 checksum of A and B concatenated.
"""

def decompress(data: ReadableBuffer, /, wbits: int = 15, bufsize: int = 16384) -> bytes:
    """Returns a bytes object containing the uncompressed data.

  data
    Compressed data.
  wbits
    The window buffer size and container format.
  bufsize
    The initial output buffer size.
"""
def decompressobj(wbits: int = 15, zdict: ReadableBuffer = b"") -> _Decompress:
    """Return a decompressor object.

  wbits
    The window buffer size and container format.
  zdict
    The predefined compression dictionary.  This must be the same
    dictionary as used by the compressor that produced the input data.
"""

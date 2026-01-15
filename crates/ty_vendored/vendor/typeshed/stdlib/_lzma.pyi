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
FILTER_LZMA1: Final[int]  # v big number
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
PRESET_EXTREME: Final[int]  # v big number

@final
class LZMADecompressor:
    """Create a decompressor object for decompressing data incrementally.

      format
        Specifies the container format of the input stream.  If this is
        FORMAT_AUTO (the default), the decompressor will automatically detect
        whether the input is FORMAT_XZ or FORMAT_ALONE.  Streams created with
        FORMAT_RAW cannot be autodetected.
      memlimit
        Limit the amount of memory used by the decompressor.  This will cause
        decompression to fail if the input cannot be decompressed within the
        given limit.
      filters
        A custom filter chain.  This argument is required for FORMAT_RAW, and
        not accepted with any other format.  When provided, this should be a
        sequence of dicts, each indicating the ID and options for a single
        filter.

    For one-shot decompression, use the decompress() function instead.
    """

    if sys.version_info >= (3, 12):
        def __new__(cls, format: int = 0, memlimit: int | None = None, filters: _FilterChain | None = None) -> Self: ...
    else:
        def __init__(self, format: int = 0, memlimit: int | None = None, filters: _FilterChain | None = None) -> None: ...

    def decompress(self, data: ReadableBuffer, max_length: int = -1) -> bytes:
        """Decompress *data*, returning uncompressed data as bytes.

        If *max_length* is nonnegative, returns at most *max_length* bytes of
        decompressed data. If this limit is reached and further output can be
        produced, *self.needs_input* will be set to ``False``. In this case, the next
        call to *decompress()* may provide *data* as b'' to obtain more of the output.

        If all of the input data was decompressed and returned (either because this
        was less than *max_length* bytes, or because *max_length* was negative),
        *self.needs_input* will be set to True.

        Attempting to decompress data after the end of stream is reached raises an
        EOFError.  Any data found after the end of the stream is ignored and saved in
        the unused_data attribute.
        """

    @property
    def check(self) -> int:
        """ID of the integrity check used by the input stream."""

    @property
    def eof(self) -> bool:
        """True if the end-of-stream marker has been reached."""

    @property
    def unused_data(self) -> bytes:
        """Data found after the end of the compressed stream."""

    @property
    def needs_input(self) -> bool:
        """True if more input is needed before more decompressed data can be produced."""

@final
class LZMACompressor:
    """LZMACompressor(format=FORMAT_XZ, check=-1, preset=None, filters=None)

    Create a compressor object for compressing data incrementally.

    format specifies the container format to use for the output. This can
    be FORMAT_XZ (default), FORMAT_ALONE, or FORMAT_RAW.

    check specifies the integrity check to use. For FORMAT_XZ, the default
    is CHECK_CRC64. FORMAT_ALONE and FORMAT_RAW do not support integrity
    checks; for these formats, check must be omitted, or be CHECK_NONE.

    The settings used by the compressor can be specified either as a
    preset compression level (with the 'preset' argument), or in detail
    as a custom filter chain (with the 'filters' argument). For FORMAT_XZ
    and FORMAT_ALONE, the default is to use the PRESET_DEFAULT preset
    level. For FORMAT_RAW, the caller must always specify a filter chain;
    the raw compressor does not support preset compression levels.

    preset (if provided) should be an integer in the range 0-9, optionally
    OR-ed with the constant PRESET_EXTREME.

    filters (if provided) should be a sequence of dicts. Each dict should
    have an entry for "id" indicating the ID of the filter, plus
    additional entries for options to the filter.

    For one-shot compression, use the compress() function instead.
    """

    if sys.version_info >= (3, 12):
        def __new__(
            cls, format: int = 1, check: int = -1, preset: int | None = None, filters: _FilterChain | None = None
        ) -> Self: ...
    else:
        def __init__(
            self, format: int = 1, check: int = -1, preset: int | None = None, filters: _FilterChain | None = None
        ) -> None: ...

    def compress(self, data: ReadableBuffer, /) -> bytes:
        """Provide data to the compressor object.

        Returns a chunk of compressed data if possible, or b'' otherwise.

        When you have finished providing data to the compressor, call the
        flush() method to finish the compression process.
        """

    def flush(self) -> bytes:
        """Finish the compression process.

        Returns the compressed data left in internal buffers.

        The compressor object may not be used after this method is called.
        """

class LZMAError(Exception):
    """Call to liblzma failed."""

def is_check_supported(check_id: int, /) -> bool:
    """Test whether the given integrity check is supported.

    Always returns True for CHECK_NONE and CHECK_CRC32.
    """

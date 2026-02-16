"""Implementation module for Zstandard compression."""

from _typeshed import ReadableBuffer
from collections.abc import Mapping
from compression.zstd import CompressionParameter, DecompressionParameter
from typing import Final, Literal, final
from typing_extensions import Self, TypeAlias

ZSTD_CLEVEL_DEFAULT: Final = 3
ZSTD_DStreamOutSize: Final = 131072
ZSTD_btlazy2: Final = 6
ZSTD_btopt: Final = 7
ZSTD_btultra: Final = 8
ZSTD_btultra2: Final = 9
ZSTD_c_chainLog: Final = 103
ZSTD_c_checksumFlag: Final = 201
ZSTD_c_compressionLevel: Final = 100
ZSTD_c_contentSizeFlag: Final = 200
ZSTD_c_dictIDFlag: Final = 202
ZSTD_c_enableLongDistanceMatching: Final = 160
ZSTD_c_hashLog: Final = 102
ZSTD_c_jobSize: Final = 401
ZSTD_c_ldmBucketSizeLog: Final = 163
ZSTD_c_ldmHashLog: Final = 161
ZSTD_c_ldmHashRateLog: Final = 164
ZSTD_c_ldmMinMatch: Final = 162
ZSTD_c_minMatch: Final = 105
ZSTD_c_nbWorkers: Final = 400
ZSTD_c_overlapLog: Final = 402
ZSTD_c_searchLog: Final = 104
ZSTD_c_strategy: Final = 107
ZSTD_c_targetLength: Final = 106
ZSTD_c_windowLog: Final = 101
ZSTD_d_windowLogMax: Final = 100
ZSTD_dfast: Final = 2
ZSTD_fast: Final = 1
ZSTD_greedy: Final = 3
ZSTD_lazy: Final = 4
ZSTD_lazy2: Final = 5

_ZstdCompressorContinue: TypeAlias = Literal[0]
_ZstdCompressorFlushBlock: TypeAlias = Literal[1]
_ZstdCompressorFlushFrame: TypeAlias = Literal[2]

@final
class ZstdCompressor:
    """Create a compressor object for compressing data incrementally.

      level
        The compression level to use. Defaults to COMPRESSION_LEVEL_DEFAULT.
      options
        A dict object that contains advanced compression parameters.
      zstd_dict
        A ZstdDict object, a pre-trained Zstandard dictionary.

    Thread-safe at method level. For one-shot compression, use the compress()
    function instead.
    """

    CONTINUE: Final = 0
    FLUSH_BLOCK: Final = 1
    FLUSH_FRAME: Final = 2
    def __new__(
        cls,
        level: int | None = None,
        options: Mapping[int, int] | None = None,
        zstd_dict: ZstdDict | tuple[ZstdDict, int] | None = None,
    ) -> Self: ...
    def compress(
        self, /, data: ReadableBuffer, mode: _ZstdCompressorContinue | _ZstdCompressorFlushBlock | _ZstdCompressorFlushFrame = 0
    ) -> bytes:
        """Provide data to the compressor object.

          mode
            Can be these 3 values ZstdCompressor.CONTINUE,
            ZstdCompressor.FLUSH_BLOCK, ZstdCompressor.FLUSH_FRAME

        Return a chunk of compressed data if possible, or b'' otherwise. When you have
        finished providing data to the compressor, call the flush() method to finish
        the compression process.
        """

    def flush(self, /, mode: _ZstdCompressorFlushBlock | _ZstdCompressorFlushFrame = 2) -> bytes:
        """Finish the compression process.

          mode
            Can be these 2 values ZstdCompressor.FLUSH_FRAME,
            ZstdCompressor.FLUSH_BLOCK

        Flush any remaining data left in internal buffers. Since Zstandard data
        consists of one or more independent frames, the compressor object can still
        be used after this method is called.
        """

    def set_pledged_input_size(self, size: int | None, /) -> None:
        """Set the uncompressed content size to be written into the frame header.

          size
            The size of the uncompressed data to be provided to the compressor.

        This method can be used to ensure the header of the frame about to be written
        includes the size of the data, unless the CompressionParameter.content_size_flag
        is set to False. If last_mode != FLUSH_FRAME, then a RuntimeError is raised.

        It is important to ensure that the pledged data size matches the actual data
        size. If they do not match the compressed output data may be corrupted and the
        final chunk written may be lost.
        """

    @property
    def last_mode(self) -> _ZstdCompressorContinue | _ZstdCompressorFlushBlock | _ZstdCompressorFlushFrame:
        """The last mode used to this compressor object, its value can be .CONTINUE,
        .FLUSH_BLOCK, .FLUSH_FRAME. Initialized to .FLUSH_FRAME.

        It can be used to get the current state of a compressor, such as, data
        flushed, or a frame ended.
        """

@final
class ZstdDecompressor:
    """Create a decompressor object for decompressing data incrementally.

      zstd_dict
        A ZstdDict object, a pre-trained Zstandard dictionary.
      options
        A dict object that contains advanced decompression parameters.

    Thread-safe at method level. For one-shot decompression, use the decompress()
    function instead.
    """

    def __new__(
        cls, zstd_dict: ZstdDict | tuple[ZstdDict, int] | None = None, options: Mapping[int, int] | None = None
    ) -> Self: ...
    def decompress(self, /, data: ReadableBuffer, max_length: int = -1) -> bytes:
        """Decompress *data*, returning uncompressed bytes if possible, or b'' otherwise.

          data
            A bytes-like object, Zstandard data to be decompressed.
          max_length
            Maximum size of returned data. When it is negative, the size of
            output buffer is unlimited. When it is nonnegative, returns at
            most max_length bytes of decompressed data.

        If *max_length* is nonnegative, returns at most *max_length* bytes of
        decompressed data. If this limit is reached and further output can be
        produced, *self.needs_input* will be set to ``False``. In this case, the next
        call to *decompress()* may provide *data* as b'' to obtain more of the output.

        If all of the input data was decompressed and returned (either because this
        was less than *max_length* bytes, or because *max_length* was negative),
        *self.needs_input* will be set to True.

        Attempting to decompress data after the end of a frame is reached raises an
        EOFError. Any data found after the end of the frame is ignored and saved in
        the self.unused_data attribute.
        """

    @property
    def eof(self) -> bool:
        """True means the end of the first frame has been reached. If decompress data
        after that, an EOFError exception will be raised.
        """

    @property
    def needs_input(self) -> bool:
        """If the max_length output limit in .decompress() method has been reached,
        and the decompressor has (or may has) unconsumed input data, it will be set
        to False. In this case, passing b'' to the .decompress() method may output
        further data.
        """

    @property
    def unused_data(self) -> bytes:
        """A bytes object of un-consumed input data.

        When ZstdDecompressor object stops after a frame is
        decompressed, unused input data after the frame. Otherwise this will be b''.
        """

@final
class ZstdDict:
    """Represents a Zstandard dictionary.

      dict_content
        The content of a Zstandard dictionary as a bytes-like object.
      is_raw
        If true, perform no checks on *dict_content*, useful for some
        advanced cases. Otherwise, check that the content represents
        a Zstandard dictionary created by the zstd library or CLI.

    The dictionary can be used for compression or decompression, and can be shared
    by multiple ZstdCompressor or ZstdDecompressor objects.
    """

    def __new__(cls, dict_content: bytes, /, *, is_raw: bool = False) -> Self: ...
    def __len__(self, /) -> int:
        """Return len(self)."""

    @property
    def as_digested_dict(self) -> tuple[Self, int]:
        """Load as a digested dictionary to compressor.

        Pass this attribute as zstd_dict argument:
        compress(dat, zstd_dict=zd.as_digested_dict)

        1. Some advanced compression parameters of compressor may be overridden
           by parameters of digested dictionary.
        2. ZstdDict has a digested dictionaries cache for each compression level.
           It's faster when loading again a digested dictionary with the same
           compression level.
        3. No need to use this for decompression.
        """

    @property
    def as_prefix(self) -> tuple[Self, int]:
        """Load as a prefix to compressor/decompressor.

        Pass this attribute as zstd_dict argument:
        compress(dat, zstd_dict=zd.as_prefix)

        1. Prefix is compatible with long distance matching, while dictionary is not.
        2. It only works for the first frame, then the compressor/decompressor will
           return to no prefix state.
        3. When decompressing, must use the same prefix as when compressing.
        """

    @property
    def as_undigested_dict(self) -> tuple[Self, int]:
        """Load as an undigested dictionary to compressor.

        Pass this attribute as zstd_dict argument:
        compress(dat, zstd_dict=zd.as_undigested_dict)

        1. The advanced compression parameters of compressor will not be overridden.
        2. Loading an undigested dictionary is costly. If load an undigested dictionary
           multiple times, consider reusing a compressor object.
        3. No need to use this for decompression.
        """

    @property
    def dict_content(self) -> bytes:
        """The content of a Zstandard dictionary, as a bytes object."""

    @property
    def dict_id(self) -> int:
        """The Zstandard dictionary, an int between 0 and 2**32.

        A non-zero value represents an ordinary Zstandard dictionary,
        conforming to the standardised format.

        A value of zero indicates a 'raw content' dictionary,
        without any restrictions on format or content.
        """

class ZstdError(Exception):
    """An error occurred in the zstd library."""

def finalize_dict(
    custom_dict_bytes: bytes, samples_bytes: bytes, samples_sizes: tuple[int, ...], dict_size: int, compression_level: int, /
) -> bytes:
    """Finalize a Zstandard dictionary.

    custom_dict_bytes
      Custom dictionary content.
    samples_bytes
      Concatenation of samples.
    samples_sizes
      Tuple of samples' sizes.
    dict_size
      The size of the dictionary.
    compression_level
      Optimize for a specific Zstandard compression level, 0 means default.
    """

def get_frame_info(frame_buffer: ReadableBuffer) -> tuple[int, int]:
    """Get Zstandard frame infomation from a frame header.

    frame_buffer
      A bytes-like object, containing the header of a Zstandard frame.
    """

def get_frame_size(frame_buffer: ReadableBuffer) -> int:
    """Get the size of a Zstandard frame, including the header and optional checksum.

    frame_buffer
      A bytes-like object, it should start from the beginning of a frame,
      and contains at least one complete frame.
    """

def get_param_bounds(parameter: int, is_compress: bool) -> tuple[int, int]:
    """Get CompressionParameter/DecompressionParameter bounds.

    parameter
      The parameter to get bounds.
    is_compress
      True for CompressionParameter, False for DecompressionParameter.
    """

def set_parameter_types(c_parameter_type: type[CompressionParameter], d_parameter_type: type[DecompressionParameter]) -> None:
    """Set CompressionParameter and DecompressionParameter types for validity check.

    c_parameter_type
      CompressionParameter IntEnum type object
    d_parameter_type
      DecompressionParameter IntEnum type object
    """

def train_dict(samples_bytes: bytes, samples_sizes: tuple[int, ...], dict_size: int, /) -> bytes:
    """Train a Zstandard dictionary on sample data.

    samples_bytes
      Concatenation of samples.
    samples_sizes
      Tuple of samples' sizes.
    dict_size
      The size of the dictionary.
    """

zstd_version: Final[str]
zstd_version_number: Final[int]

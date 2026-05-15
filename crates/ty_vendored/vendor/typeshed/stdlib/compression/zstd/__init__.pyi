"""Python bindings to the Zstandard (zstd) compression library (RFC-8878)."""

import enum
from _typeshed import ReadableBuffer
from collections.abc import Iterable, Mapping
from compression.zstd._zstdfile import ZstdFile, open
from typing import Final, final

import _zstd
from _zstd import ZstdCompressor, ZstdDecompressor, ZstdDict, ZstdError, get_frame_size, zstd_version

__all__ = (
    # compression.zstd
    "COMPRESSION_LEVEL_DEFAULT",
    "compress",
    "CompressionParameter",
    "decompress",
    "DecompressionParameter",
    "finalize_dict",
    "get_frame_info",
    "Strategy",
    "train_dict",
    # compression.zstd._zstdfile
    "open",
    "ZstdFile",
    # _zstd
    "get_frame_size",
    "zstd_version",
    "zstd_version_info",
    "ZstdCompressor",
    "ZstdDecompressor",
    "ZstdDict",
    "ZstdError",
)

zstd_version_info: Final[tuple[int, int, int]]
COMPRESSION_LEVEL_DEFAULT: Final = _zstd.ZSTD_CLEVEL_DEFAULT

class FrameInfo:
    """Information about a Zstandard frame."""

    __slots__ = ("decompressed_size", "dictionary_id")
    decompressed_size: int
    dictionary_id: int
    def __init__(self, decompressed_size: int, dictionary_id: int) -> None: ...

def get_frame_info(frame_buffer: ReadableBuffer) -> FrameInfo:
    """Get Zstandard frame information from a frame header.

    *frame_buffer* is a bytes-like object. It should start from the beginning
    of a frame, and needs to include at least the frame header (6 to 18 bytes).

    The returned FrameInfo object has two attributes.
    'decompressed_size' is the size in bytes of the data in the frame when
    decompressed, or None when the decompressed size is unknown.
    'dictionary_id' is an int in the range (0, 2**32). The special value 0
    means that the dictionary ID was not recorded in the frame header,
    the frame may or may not need a dictionary to be decoded,
    and the ID of such a dictionary is not specified.
    """

def train_dict(samples: Iterable[ReadableBuffer], dict_size: int) -> ZstdDict:
    """Return a ZstdDict representing a trained Zstandard dictionary.

    *samples* is an iterable of samples, where a sample is a bytes-like
    object representing a file.

    *dict_size* is the dictionary's maximum size, in bytes.
    """

def finalize_dict(zstd_dict: ZstdDict, /, samples: Iterable[ReadableBuffer], dict_size: int, level: int) -> ZstdDict:
    """Return a ZstdDict representing a finalized Zstandard dictionary.

    Given a custom content as a basis for dictionary, and a set of samples,
    finalize *zstd_dict* by adding headers and statistics according to the
    Zstandard dictionary format.

    You may compose an effective dictionary content by hand, which is used as
    basis dictionary, and use some samples to finalize a dictionary. The basis
    dictionary may be a "raw content" dictionary. See *is_raw* in ZstdDict.

    *samples* is an iterable of samples, where a sample is a bytes-like object
    representing a file.
    *dict_size* is the dictionary's maximum size, in bytes.
    *level* is the expected compression level. The statistics for each
    compression level differ, so tuning the dictionary to the compression level
    can provide improvements.
    """

def compress(
    data: ReadableBuffer,
    level: int | None = None,
    options: Mapping[int, int] | None = None,
    zstd_dict: ZstdDict | tuple[ZstdDict, int] | None = None,
) -> bytes:
    """Return Zstandard compressed *data* as bytes.

    *level* is an int specifying the compression level to use, defaulting to
    COMPRESSION_LEVEL_DEFAULT ('3').
    *options* is a dict object that contains advanced compression
    parameters. See CompressionParameter for more on options.
    *zstd_dict* is a ZstdDict object, a pre-trained Zstandard dictionary. See
    the function train_dict for how to train a ZstdDict on sample data.

    For incremental compression, use a ZstdCompressor instead.
    """

def decompress(
    data: ReadableBuffer, zstd_dict: ZstdDict | tuple[ZstdDict, int] | None = None, options: Mapping[int, int] | None = None
) -> bytes:
    """Decompress one or more frames of Zstandard compressed *data*.

    *zstd_dict* is a ZstdDict object, a pre-trained Zstandard dictionary. See
    the function train_dict for how to train a ZstdDict on sample data.
    *options* is a dict object that contains advanced compression
    parameters. See DecompressionParameter for more on options.

    For incremental decompression, use a ZstdDecompressor instead.
    """

@final
class CompressionParameter(enum.IntEnum):
    """Compression parameters."""

    compression_level = _zstd.ZSTD_c_compressionLevel
    window_log = _zstd.ZSTD_c_windowLog
    hash_log = _zstd.ZSTD_c_hashLog
    chain_log = _zstd.ZSTD_c_chainLog
    search_log = _zstd.ZSTD_c_searchLog
    min_match = _zstd.ZSTD_c_minMatch
    target_length = _zstd.ZSTD_c_targetLength
    strategy = _zstd.ZSTD_c_strategy
    enable_long_distance_matching = _zstd.ZSTD_c_enableLongDistanceMatching
    ldm_hash_log = _zstd.ZSTD_c_ldmHashLog
    ldm_min_match = _zstd.ZSTD_c_ldmMinMatch
    ldm_bucket_size_log = _zstd.ZSTD_c_ldmBucketSizeLog
    ldm_hash_rate_log = _zstd.ZSTD_c_ldmHashRateLog
    content_size_flag = _zstd.ZSTD_c_contentSizeFlag
    checksum_flag = _zstd.ZSTD_c_checksumFlag
    dict_id_flag = _zstd.ZSTD_c_dictIDFlag
    nb_workers = _zstd.ZSTD_c_nbWorkers
    job_size = _zstd.ZSTD_c_jobSize
    overlap_log = _zstd.ZSTD_c_overlapLog
    def bounds(self) -> tuple[int, int]:
        """Return the (lower, upper) int bounds of a compression parameter.

        Both the lower and upper bounds are inclusive.
        """

@final
class DecompressionParameter(enum.IntEnum):
    """Decompression parameters."""

    window_log_max = _zstd.ZSTD_d_windowLogMax
    def bounds(self) -> tuple[int, int]:
        """Return the (lower, upper) int bounds of a decompression parameter.

        Both the lower and upper bounds are inclusive.
        """

@final
class Strategy(enum.IntEnum):
    """Compression strategies, listed from fastest to strongest.

    Note that new strategies might be added in the future.
    Only the order (from fast to strong) is guaranteed,
    the numeric value might change.
    """

    fast = _zstd.ZSTD_fast
    dfast = _zstd.ZSTD_dfast
    greedy = _zstd.ZSTD_greedy
    lazy = _zstd.ZSTD_lazy
    lazy2 = _zstd.ZSTD_lazy2
    btlazy2 = _zstd.ZSTD_btlazy2
    btopt = _zstd.ZSTD_btopt
    btultra = _zstd.ZSTD_btultra
    btultra2 = _zstd.ZSTD_btultra2

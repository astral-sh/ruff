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
    CONTINUE: Final = 0
    FLUSH_BLOCK: Final = 1
    FLUSH_FRAME: Final = 2
    def __init__(
        self, level: int | None = None, options: Mapping[int, int] | None = None, zstd_dict: ZstdDict | None = None
    ) -> None: ...
    def compress(
        self, /, data: ReadableBuffer, mode: _ZstdCompressorContinue | _ZstdCompressorFlushBlock | _ZstdCompressorFlushFrame = 0
    ) -> bytes: ...
    def flush(self, /, mode: _ZstdCompressorFlushBlock | _ZstdCompressorFlushFrame = 2) -> bytes: ...
    @property
    def last_mode(self) -> _ZstdCompressorContinue | _ZstdCompressorFlushBlock | _ZstdCompressorFlushFrame: ...

@final
class ZstdDecompressor:
    def __init__(self, zstd_dict: ZstdDict | None = None, options: Mapping[int, int] | None = None) -> None: ...
    def decompress(self, /, data: ReadableBuffer, max_length: int = -1) -> bytes: ...
    @property
    def eof(self) -> bool: ...
    @property
    def needs_input(self) -> bool: ...
    @property
    def unused_data(self) -> bytes: ...

@final
class ZstdDict:
    def __init__(self, dict_content: bytes, /, *, is_raw: bool = False) -> None: ...
    def __len__(self, /) -> int: ...
    @property
    def as_digested_dict(self) -> tuple[Self, int]: ...
    @property
    def as_prefix(self) -> tuple[Self, int]: ...
    @property
    def as_undigested_dict(self) -> tuple[Self, int]: ...
    @property
    def dict_content(self) -> bytes: ...
    @property
    def dict_id(self) -> int: ...

class ZstdError(Exception): ...

def finalize_dict(
    custom_dict_bytes: bytes, samples_bytes: bytes, samples_sizes: tuple[int, ...], dict_size: int, compression_level: int, /
) -> bytes: ...
def get_frame_info(frame_buffer: ReadableBuffer) -> tuple[int, int]: ...
def get_frame_size(frame_buffer: ReadableBuffer) -> int: ...
def get_param_bounds(parameter: int, is_compress: bool) -> tuple[int, int]: ...
def set_parameter_types(c_parameter_type: type[CompressionParameter], d_parameter_type: type[DecompressionParameter]) -> None: ...
def train_dict(samples_bytes: bytes, samples_sizes: tuple[int, ...], dict_size: int, /) -> bytes: ...

zstd_version: Final[str]
zstd_version_number: Final[int]

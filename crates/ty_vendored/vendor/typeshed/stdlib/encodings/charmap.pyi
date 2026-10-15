import codecs
from _codecs import _DecodeCharMap, _EncodeCharMap
from _typeshed import ReadableBuffer

class Codec(codecs.Codec):
    # At runtime, this is codecs.charmap_encode
    @staticmethod
    def encode(str: str, errors: str | None = None, mapping: _EncodeCharMap | None = None, /) -> tuple[bytes, int]: ...
    # At runtime, this is codecs.charmap_decode
    @staticmethod
    def decode(data: ReadableBuffer, errors: str | None = None, mapping: _DecodeCharMap | None = None, /) -> tuple[str, int]: ...

class IncrementalEncoder(codecs.IncrementalEncoder):
    mapping: _EncodeCharMap | None
    def __init__(self, errors: str = "strict", mapping: _EncodeCharMap | None = None) -> None: ...
    def encode(self, input: str, final: bool = False) -> bytes: ...

class IncrementalDecoder(codecs.IncrementalDecoder):
    mapping: _DecodeCharMap | None
    def __init__(self, errors: str = "strict", mapping: _DecodeCharMap | None = None) -> None: ...
    def decode(self, input: ReadableBuffer, final: bool = False) -> str: ...

class StreamWriter(Codec, codecs.StreamWriter):
    mapping: _EncodeCharMap | None
    def __init__(self, stream: codecs._WritableStream, errors: str = "strict", mapping: _EncodeCharMap | None = None) -> None: ...
    def encode(self, input: str, errors: str = "strict") -> tuple[bytes, int]: ...  # type: ignore[override]

class StreamReader(Codec, codecs.StreamReader):
    mapping: _DecodeCharMap | None
    def __init__(self, stream: codecs._ReadableStream, errors: str = "strict", mapping: _DecodeCharMap | None = None) -> None: ...
    def decode(self, input: ReadableBuffer, errors: str = "strict") -> tuple[str, int]: ...  # type: ignore[override]

def getregentry() -> codecs.CodecInfo: ...

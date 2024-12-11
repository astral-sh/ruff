import codecs
import sys
from _typeshed import ReadableBuffer

class Codec(codecs.Codec):
    # At runtime, this is codecs.unicode_escape_encode
    @staticmethod
    def encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
    # At runtime, this is codecs.unicode_escape_decode
    if sys.version_info >= (3, 9):
        @staticmethod
        def decode(data: str | ReadableBuffer, errors: str | None = None, final: bool = True, /) -> tuple[str, int]: ...
    else:
        @staticmethod
        def decode(data: str | ReadableBuffer, errors: str | None = None, /) -> tuple[str, int]: ...

class IncrementalEncoder(codecs.IncrementalEncoder):
    def encode(self, input: str, final: bool = False) -> bytes: ...

if sys.version_info >= (3, 9):
    class IncrementalDecoder(codecs.BufferedIncrementalDecoder):
        def _buffer_decode(self, input: str | ReadableBuffer, errors: str | None, final: bool) -> tuple[str, int]: ...

else:
    class IncrementalDecoder(codecs.IncrementalDecoder):
        def decode(self, input: str | ReadableBuffer, final: bool = False) -> str: ...

class StreamWriter(Codec, codecs.StreamWriter): ...

class StreamReader(Codec, codecs.StreamReader):
    if sys.version_info >= (3, 9):
        def decode(self, input: str | ReadableBuffer, errors: str = "strict") -> tuple[str, int]: ...  # type: ignore[override]

def getregentry() -> codecs.CodecInfo: ...

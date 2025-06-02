import codecs
from _typeshed import ReadableBuffer
from typing import ClassVar

# This codec is bytes to bytes.

def base64_encode(input: ReadableBuffer, errors: str = "strict") -> tuple[bytes, int]: ...
def base64_decode(input: ReadableBuffer, errors: str = "strict") -> tuple[bytes, int]: ...

class Codec(codecs.Codec):
    def encode(self, input: ReadableBuffer, errors: str = "strict") -> tuple[bytes, int]: ...  # type: ignore[override]
    def decode(self, input: ReadableBuffer, errors: str = "strict") -> tuple[bytes, int]: ...  # type: ignore[override]

class IncrementalEncoder(codecs.IncrementalEncoder):
    def encode(self, input: ReadableBuffer, final: bool = False) -> bytes: ...  # type: ignore[override]

class IncrementalDecoder(codecs.IncrementalDecoder):
    def decode(self, input: ReadableBuffer, final: bool = False) -> bytes: ...  # type: ignore[override]

class StreamWriter(Codec, codecs.StreamWriter):
    charbuffertype: ClassVar[type] = ...

class StreamReader(Codec, codecs.StreamReader):
    charbuffertype: ClassVar[type] = ...

def getregentry() -> codecs.CodecInfo: ...

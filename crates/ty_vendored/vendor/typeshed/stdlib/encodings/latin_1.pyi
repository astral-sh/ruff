import codecs
from _typeshed import ReadableBuffer

class Codec(codecs.Codec):
    # At runtime, this is codecs.latin_1_encode
    @staticmethod
    def encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
    # At runtime, this is codecs.latin_1_decode
    @staticmethod
    def decode(data: ReadableBuffer, errors: str | None = None, /) -> tuple[str, int]: ...

class IncrementalEncoder(codecs.IncrementalEncoder):
    def encode(self, input: str, final: bool = False) -> bytes: ...

class IncrementalDecoder(codecs.IncrementalDecoder):
    def decode(self, input: ReadableBuffer, final: bool = False) -> str: ...

class StreamWriter(Codec, codecs.StreamWriter): ...
class StreamReader(Codec, codecs.StreamReader): ...

# Note: encode being a decode function and decode being an encode function is accurate to runtime.
class StreamConverter(StreamWriter, StreamReader):  # type: ignore[misc]  # incompatible methods in base classes
    # At runtime, this is codecs.latin_1_decode
    @staticmethod
    def encode(data: ReadableBuffer, errors: str | None = None, /) -> tuple[str, int]: ...  # type: ignore[override]
    # At runtime, this is codecs.latin_1_encode
    @staticmethod
    def decode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...  # type: ignore[override]

def getregentry() -> codecs.CodecInfo: ...

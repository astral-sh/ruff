"""Python 'raw-unicode-escape' Codec


Written by Marc-Andre Lemburg (mal@lemburg.com).

(c) Copyright CNRI, All Rights Reserved. NO WARRANTY.

"""

import codecs
from _typeshed import ReadableBuffer

class Codec(codecs.Codec):
    # At runtime, this is codecs.raw_unicode_escape_encode
    @staticmethod
    def encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
    # At runtime, this is codecs.raw_unicode_escape_decode
    @staticmethod
    def decode(data: str | ReadableBuffer, errors: str | None = None, final: bool = True, /) -> tuple[str, int]: ...

class IncrementalEncoder(codecs.IncrementalEncoder):
    def encode(self, input: str, final: bool = False) -> bytes: ...

class IncrementalDecoder(codecs.BufferedIncrementalDecoder):
    def _buffer_decode(self, input: str | ReadableBuffer, errors: str | None, final: bool) -> tuple[str, int]: ...

class StreamWriter(Codec, codecs.StreamWriter): ...

class StreamReader(Codec, codecs.StreamReader):
    def decode(self, input: str | ReadableBuffer, errors: str = "strict") -> tuple[str, int]: ...  # type: ignore[override]

def getregentry() -> codecs.CodecInfo: ...

"""Python 'undefined' Codec

    This codec will always raise a UnicodeError exception when being
    used. It is intended for use by the site.py file to switch off
    automatic string to Unicode coercion.

Written by Marc-Andre Lemburg (mal@lemburg.com).

(c) Copyright CNRI, All Rights Reserved. NO WARRANTY.

"""

import codecs
from _typeshed import ReadableBuffer

# These return types are just to match the base types. In reality, these always
# raise an error.

class Codec(codecs.Codec):
    def encode(self, input: str, errors: str = "strict") -> tuple[bytes, int]: ...
    def decode(self, input: ReadableBuffer, errors: str = "strict") -> tuple[str, int]: ...

class IncrementalEncoder(codecs.IncrementalEncoder):
    def encode(self, input: str, final: bool = False) -> bytes: ...

class IncrementalDecoder(codecs.IncrementalDecoder):
    def decode(self, input: ReadableBuffer, final: bool = False) -> str: ...

class StreamWriter(Codec, codecs.StreamWriter): ...
class StreamReader(Codec, codecs.StreamReader): ...

def getregentry() -> codecs.CodecInfo: ...

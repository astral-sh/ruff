import _multibytecodec as mbc
import codecs
from typing import ClassVar

codec: mbc._MultibyteCodec

class Codec(codecs.Codec):
    encode = codec.encode  # type: ignore[assignment]  # pyright: ignore[reportAssignmentType]
    decode = codec.decode  # type: ignore[assignment]  # pyright: ignore[reportAssignmentType]

class IncrementalEncoder(mbc.MultibyteIncrementalEncoder, codecs.IncrementalEncoder):  # type: ignore[misc]
    codec: ClassVar[mbc._MultibyteCodec] = ...

class IncrementalDecoder(mbc.MultibyteIncrementalDecoder, codecs.IncrementalDecoder):
    codec: ClassVar[mbc._MultibyteCodec] = ...

class StreamReader(Codec, mbc.MultibyteStreamReader, codecs.StreamReader):  # type: ignore[misc]
    codec: ClassVar[mbc._MultibyteCodec] = ...

class StreamWriter(Codec, mbc.MultibyteStreamWriter, codecs.StreamWriter):
    codec: ClassVar[mbc._MultibyteCodec] = ...

def getregentry() -> codecs.CodecInfo: ...

"""Python Character Mapping Codec generated from 'hp_roman8.txt' with gencodec.py.

Based on data from ftp://dkuug.dk/i18n/charmaps/HP-ROMAN8 (Keld Simonsen)

Original source: LaserJet IIP Printer User's Manual HP part no
33471-90901, Hewlet-Packard, June 1989.

(Used with permission)

"""

import codecs
from _codecs import _EncodingMap
from _typeshed import ReadableBuffer

class Codec(codecs.Codec):
    def encode(self, input: str, errors: str = "strict") -> tuple[bytes, int]: ...
    def decode(self, input: bytes, errors: str = "strict") -> tuple[str, int]: ...

class IncrementalEncoder(codecs.IncrementalEncoder):
    def encode(self, input: str, final: bool = False) -> bytes: ...

class IncrementalDecoder(codecs.IncrementalDecoder):
    def decode(self, input: ReadableBuffer, final: bool = False) -> str: ...

class StreamWriter(Codec, codecs.StreamWriter): ...
class StreamReader(Codec, codecs.StreamReader): ...

def getregentry() -> codecs.CodecInfo: ...

decoding_table: str
encoding_table: _EncodingMap

import codecs
from _typeshed import SupportsRead, SupportsWrite

# This codec is string to string.

class Codec(codecs.Codec):
    def encode(self, input: str, errors: str = "strict") -> tuple[str, int]: ...  # type: ignore[override]
    def decode(self, input: str, errors: str = "strict") -> tuple[str, int]: ...  # type: ignore[override]

class IncrementalEncoder(codecs.IncrementalEncoder):
    def encode(self, input: str, final: bool = False) -> str: ...  # type: ignore[override]

class IncrementalDecoder(codecs.IncrementalDecoder):
    def decode(self, input: str, final: bool = False) -> str: ...  # type: ignore[override]

class StreamWriter(Codec, codecs.StreamWriter): ...
class StreamReader(Codec, codecs.StreamReader): ...

def getregentry() -> codecs.CodecInfo: ...

rot13_map: dict[int, int]

def rot13(infile: SupportsRead[str], outfile: SupportsWrite[str]) -> None: ...

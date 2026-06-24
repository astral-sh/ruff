"""Python 'mbcs' Codec for Windows


Cloned by Mark Hammond (mhammond@skippinet.com.au) from ascii.py,
which was written by Marc-Andre Lemburg (mal@lemburg.com).

(c) Copyright CNRI, All Rights Reserved. NO WARRANTY.

"""

import codecs
import sys
from _typeshed import ReadableBuffer

if sys.platform == "win32":
    encode = codecs.mbcs_encode

    def decode(input: ReadableBuffer, errors: str | None = "strict") -> tuple[str, int]: ...

    class IncrementalEncoder(codecs.IncrementalEncoder):
        def encode(self, input: str, final: bool = False) -> bytes: ...

    class IncrementalDecoder(codecs.BufferedIncrementalDecoder):
        # At runtime, this is codecs.mbcs_decode
        @staticmethod
        def _buffer_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...

    class StreamWriter(codecs.StreamWriter):
        # At runtime, this is codecs.mbcs_encode
        @staticmethod
        def encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...

    class StreamReader(codecs.StreamReader):
        # At runtime, this is codecs.mbcs_decode
        @staticmethod
        def decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...

    def getregentry() -> codecs.CodecInfo: ...

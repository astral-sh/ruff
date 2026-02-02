"""Codec for the Punycode encoding, as specified in RFC 3492

Written by Martin v. Löwis.
"""

import codecs
from typing import Literal

def segregate(str: str) -> tuple[bytes, list[int]]:
    """3.1 Basic code point segregation"""

def selective_len(str: str, max: int) -> int:
    """Return the length of str, considering only characters below max."""

def selective_find(str: str, char: str, index: int, pos: int) -> tuple[int, int]:
    """Return a pair (index, pos), indicating the next occurrence of
    char in str. index is the position of the character considering
    only ordinals up to and including char, and pos is the position in
    the full string. index/pos is the starting position in the full
    string.
    """

def insertion_unsort(str: str, extended: list[int]) -> list[int]:
    """3.2 Insertion unsort coding"""

def T(j: int, bias: int) -> int: ...

digits: Literal[b"abcdefghijklmnopqrstuvwxyz0123456789"]

def generate_generalized_integer(N: int, bias: int) -> bytes:
    """3.3 Generalized variable-length integers"""

def adapt(delta: int, first: bool, numchars: int) -> int: ...
def generate_integers(baselen: int, deltas: list[int]) -> bytes:
    """3.4 Bias adaptation"""

def punycode_encode(text: str) -> bytes: ...
def decode_generalized_number(extended: bytes, extpos: int, bias: int, errors: str) -> tuple[int, int | None]:
    """3.3 Generalized variable-length integers"""

def insertion_sort(base: str, extended: bytes, errors: str) -> str:
    """3.2 Insertion sort coding"""

def punycode_decode(text: memoryview | bytes | bytearray | str, errors: str) -> str: ...

class Codec(codecs.Codec):
    def encode(self, input: str, errors: str = "strict") -> tuple[bytes, int]: ...
    def decode(self, input: memoryview | bytes | bytearray | str, errors: str = "strict") -> tuple[str, int]: ...

class IncrementalEncoder(codecs.IncrementalEncoder):
    def encode(self, input: str, final: bool = False) -> bytes: ...

class IncrementalDecoder(codecs.IncrementalDecoder):
    def decode(self, input: memoryview | bytes | bytearray | str, final: bool = False) -> str: ...  # type: ignore[override]

class StreamWriter(Codec, codecs.StreamWriter): ...
class StreamReader(Codec, codecs.StreamReader): ...

def getregentry() -> codecs.CodecInfo: ...

from _typeshed import ReadableBuffer, SizedBuffer
from collections.abc import Callable
from types import ModuleType
from typing import Any, AnyStr, overload
from typing_extensions import TypeAlias

# TODO more precise type for object of hashlib
_Hash: TypeAlias = Any
_DigestMod: TypeAlias = str | Callable[[], _Hash] | ModuleType

trans_5C: bytes
trans_36: bytes

digest_size: None

# In reality digestmod has a default value, but the function always throws an error
# if the argument is not given, so we pretend it is a required argument.
@overload
def new(key: bytes | bytearray, msg: ReadableBuffer | None, digestmod: _DigestMod) -> HMAC: ...
@overload
def new(key: bytes | bytearray, *, digestmod: _DigestMod) -> HMAC: ...

class HMAC:
    digest_size: int
    block_size: int
    @property
    def name(self) -> str: ...
    def __init__(self, key: bytes | bytearray, msg: ReadableBuffer | None = None, digestmod: _DigestMod = "") -> None: ...
    def update(self, msg: ReadableBuffer) -> None: ...
    def digest(self) -> bytes: ...
    def hexdigest(self) -> str: ...
    def copy(self) -> HMAC: ...

@overload
def compare_digest(a: ReadableBuffer, b: ReadableBuffer, /) -> bool: ...
@overload
def compare_digest(a: AnyStr, b: AnyStr, /) -> bool: ...
def digest(key: SizedBuffer, msg: ReadableBuffer, digest: _DigestMod) -> bytes: ...

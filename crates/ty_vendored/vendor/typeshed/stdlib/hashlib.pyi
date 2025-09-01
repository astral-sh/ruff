import sys
from _blake2 import blake2b as blake2b, blake2s as blake2s
from _hashlib import (
    HASH,
    _HashObject,
    openssl_md5 as md5,
    openssl_sha1 as sha1,
    openssl_sha3_224 as sha3_224,
    openssl_sha3_256 as sha3_256,
    openssl_sha3_384 as sha3_384,
    openssl_sha3_512 as sha3_512,
    openssl_sha224 as sha224,
    openssl_sha256 as sha256,
    openssl_sha384 as sha384,
    openssl_sha512 as sha512,
    openssl_shake_128 as shake_128,
    openssl_shake_256 as shake_256,
    pbkdf2_hmac as pbkdf2_hmac,
    scrypt as scrypt,
)
from _typeshed import ReadableBuffer
from collections.abc import Callable, Set as AbstractSet
from typing import Protocol, type_check_only

if sys.version_info >= (3, 11):
    __all__ = (
        "md5",
        "sha1",
        "sha224",
        "sha256",
        "sha384",
        "sha512",
        "blake2b",
        "blake2s",
        "sha3_224",
        "sha3_256",
        "sha3_384",
        "sha3_512",
        "shake_128",
        "shake_256",
        "new",
        "algorithms_guaranteed",
        "algorithms_available",
        "pbkdf2_hmac",
        "file_digest",
    )
else:
    __all__ = (
        "md5",
        "sha1",
        "sha224",
        "sha256",
        "sha384",
        "sha512",
        "blake2b",
        "blake2s",
        "sha3_224",
        "sha3_256",
        "sha3_384",
        "sha3_512",
        "shake_128",
        "shake_256",
        "new",
        "algorithms_guaranteed",
        "algorithms_available",
        "pbkdf2_hmac",
    )

def new(name: str, data: ReadableBuffer = b"", *, usedforsecurity: bool = ...) -> HASH: ...

algorithms_guaranteed: AbstractSet[str]
algorithms_available: AbstractSet[str]

if sys.version_info >= (3, 11):
    @type_check_only
    class _BytesIOLike(Protocol):
        def getbuffer(self) -> ReadableBuffer: ...

    @type_check_only
    class _FileDigestFileObj(Protocol):
        def readinto(self, buf: bytearray, /) -> int: ...
        def readable(self) -> bool: ...

    def file_digest(
        fileobj: _BytesIOLike | _FileDigestFileObj, digest: str | Callable[[], _HashObject], /, *, _bufsize: int = 262144
    ) -> HASH: ...

# Legacy typing-only alias
_Hash = HASH

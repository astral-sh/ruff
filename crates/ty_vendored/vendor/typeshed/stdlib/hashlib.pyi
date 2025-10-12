"""hashlib module - A common interface to many hash functions.

new(name, data=b'', **kwargs) - returns a new hash object implementing the
                                given hash function; initializing the hash
                                using the given binary data.

Named constructor functions are also available, these are faster
than using new(name):

md5(), sha1(), sha224(), sha256(), sha384(), sha512(), blake2b(), blake2s(),
sha3_224, sha3_256, sha3_384, sha3_512, shake_128, and shake_256.

More algorithms may be available on your platform but the above are guaranteed
to exist.  See the algorithms_guaranteed and algorithms_available attributes
to find out what algorithm names can be passed to new().

NOTE: If you want the adler32 or crc32 hash functions they are available in
the zlib module.

Choose your hash function wisely.  Some have known collision weaknesses.
sha384 and sha512 will be slow on 32 bit platforms.

Hash objects have these methods:
 - update(data): Update the hash object with the bytes in data. Repeated calls
                 are equivalent to a single call with the concatenation of all
                 the arguments.
 - digest():     Return the digest of the bytes passed to the update() method
                 so far as a bytes object.
 - hexdigest():  Like digest() except the digest is returned as a string
                 of double length, containing only hexadecimal digits.
 - copy():       Return a copy (clone) of the hash object. This can be used to
                 efficiently compute the digests of data that share a common
                 initial substring.

For example, to obtain the digest of the byte string 'Nobody inspects the
spammish repetition':

    >>> import hashlib
    >>> m = hashlib.md5()
    >>> m.update(b"Nobody inspects")
    >>> m.update(b" the spammish repetition")
    >>> m.digest()
    b'\\xbbd\\x9c\\x83\\xdd\\x1e\\xa5\\xc9\\xd9\\xde\\xc9\\xa1\\x8d\\xf0\\xff\\xe9'

More condensed:

    >>> hashlib.sha224(b"Nobody inspects the spammish repetition").hexdigest()
    'a4337bc45a8fc544c03f52dc550cd6e1e87021bc896588bd79e901e2'

"""

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

def new(name: str, data: ReadableBuffer = b"", *, usedforsecurity: bool = ...) -> HASH:
    """new(name, data=b'') - Return a new hashing object using the named algorithm;
    optionally initialized with data (which must be a bytes-like object).
    """

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
    ) -> HASH:
        """Hash the contents of a file-like object. Returns a digest object.

        *fileobj* must be a file-like object opened for reading in binary mode.
        It accepts file objects from open(), io.BytesIO(), and SocketIO objects.
        The function may bypass Python's I/O and use the file descriptor *fileno*
        directly.

        *digest* must either be a hash algorithm name as a *str*, a hash
        constructor, or a callable that returns a hash object.
        """

# Legacy typing-only alias
_Hash = HASH

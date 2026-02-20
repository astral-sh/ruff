"""HMAC (Keyed-Hashing for Message Authentication) module.

Implements the HMAC algorithm as described by RFC 2104.
"""

from _hashlib import _HashObject, compare_digest as compare_digest
from _typeshed import ReadableBuffer, SizedBuffer
from collections.abc import Callable
from types import ModuleType
from typing import overload
from typing_extensions import TypeAlias

_DigestMod: TypeAlias = str | Callable[[], _HashObject] | ModuleType

trans_5C: bytes
trans_36: bytes

digest_size: None

# In reality digestmod has a default value, but the function always throws an error
# if the argument is not given, so we pretend it is a required argument.
@overload
def new(key: bytes | bytearray, msg: ReadableBuffer | None, digestmod: _DigestMod) -> HMAC:
    """Create a new hashing object and return it.

    key: bytes or buffer, The starting key for the hash.
    msg: bytes or buffer, Initial input for the hash, or None.
    digestmod: A hash name suitable for hashlib.new(). *OR*
               A hashlib constructor returning a new hash object. *OR*
               A module supporting PEP 247.

               Required as of 3.8, despite its position after the optional
               msg argument.  Passing it as a keyword argument is
               recommended, though not required for legacy API reasons.

    You can now feed arbitrary bytes into the object using its update()
    method, and can ask for the hash value at any time by calling its digest()
    or hexdigest() methods.
    """

@overload
def new(key: bytes | bytearray, *, digestmod: _DigestMod) -> HMAC: ...

class HMAC:
    """RFC 2104 HMAC class.  Also complies with RFC 4231.

    This supports the API for Cryptographic Hash Functions (PEP 247).
    """

    __slots__ = ("_hmac", "_inner", "_outer", "block_size", "digest_size")
    digest_size: int
    block_size: int
    @property
    def name(self) -> str: ...
    def __init__(self, key: bytes | bytearray, msg: ReadableBuffer | None = None, digestmod: _DigestMod = "") -> None:
        """Create a new HMAC object.

        key: bytes or buffer, key for the keyed hash object.
        msg: bytes or buffer, Initial input for the hash or None.
        digestmod: A hash name suitable for hashlib.new(). *OR*
                   A hashlib constructor returning a new hash object. *OR*
                   A module supporting PEP 247.

                   Required as of 3.8, despite its position after the optional
                   msg argument.  Passing it as a keyword argument is
                   recommended, though not required for legacy API reasons.
        """

    def update(self, msg: ReadableBuffer) -> None:
        """Feed data from msg into this hashing object."""

    def digest(self) -> bytes:
        """Return the hash value of this hashing object.

        This returns the hmac value as bytes.  The object is
        not altered in any way by this function; you can continue
        updating the object after calling this function.
        """

    def hexdigest(self) -> str:
        """Like digest(), but returns a string of hexadecimal digits instead."""

    def copy(self) -> HMAC:
        """Return a separate copy of this hashing object.

        An update to this copy won't affect the original object.
        """

def digest(key: SizedBuffer, msg: ReadableBuffer, digest: _DigestMod) -> bytes:
    """Fast inline implementation of HMAC.

    key: bytes or buffer, The key for the keyed hash object.
    msg: bytes or buffer, Input message.
    digest: A hash name suitable for hashlib.new() for best performance. *OR*
            A hashlib constructor returning a new hash object. *OR*
            A module supporting PEP 247.
    """

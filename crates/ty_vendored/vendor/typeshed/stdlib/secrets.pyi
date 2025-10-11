"""Generate cryptographically strong pseudo-random numbers suitable for
managing secrets such as account authentication, tokens, and similar.

See PEP 506 for more information.
https://peps.python.org/pep-0506/

"""

from _typeshed import SupportsLenAndGetItem
from hmac import compare_digest as compare_digest
from random import SystemRandom as SystemRandom
from typing import TypeVar

__all__ = ["choice", "randbelow", "randbits", "SystemRandom", "token_bytes", "token_hex", "token_urlsafe", "compare_digest"]

_T = TypeVar("_T")

def randbelow(exclusive_upper_bound: int) -> int:
    """Return a random int in the range [0, n)."""

def randbits(k: int) -> int:
    """getrandbits(k) -> x.  Generates an int with k random bits."""

def choice(seq: SupportsLenAndGetItem[_T]) -> _T:
    """Choose a random element from a non-empty sequence."""

def token_bytes(nbytes: int | None = None) -> bytes:
    """Return a random byte string containing *nbytes* bytes.

    If *nbytes* is ``None`` or not supplied, a reasonable
    default is used.

    >>> token_bytes(16)  #doctest:+SKIP
    b'\\xebr\\x17D*t\\xae\\xd4\\xe3S\\xb6\\xe2\\xebP1\\x8b'

    """

def token_hex(nbytes: int | None = None) -> str:
    """Return a random text string, in hexadecimal.

    The string has *nbytes* random bytes, each byte converted to two
    hex digits.  If *nbytes* is ``None`` or not supplied, a reasonable
    default is used.

    >>> token_hex(16)  #doctest:+SKIP
    'f9bf78b9a18ce6d46a0cd2b0b86df9da'

    """

def token_urlsafe(nbytes: int | None = None) -> str:
    """Return a random URL-safe text string, in Base64 encoding.

    The string has *nbytes* random bytes.  If *nbytes* is ``None``
    or not supplied, a reasonable default is used.

    >>> token_urlsafe(16)  #doctest:+SKIP
    'Drmhze6EPcv0fN_81Bj-nA'

    """

import sys
from _typeshed import ReadableBuffer
from typing import TypeAlias
from typing_extensions import deprecated

# Many functions in binascii accept buffer objects
# or ASCII-only strings.
_AsciiBuffer: TypeAlias = str | ReadableBuffer

def a2b_uu(data: _AsciiBuffer, /) -> bytes: ...
def b2a_uu(data: ReadableBuffer, /, *, backtick: bool = False) -> bytes: ...

if sys.version_info >= (3, 15):
    ASCII85_ALPHABET: bytes
    BINHEX_ALPHABET: bytes
    CRYPT_ALPHABET: bytes
    UU_ALPHABET: bytes
    BASE64_ALPHABET: bytes
    URLSAFE_BASE64_ALPHABET: bytes
    BASE32_ALPHABET: bytes
    BASE32HEX_ALPHABET: bytes
    BASE85_ALPHABET: bytes
    Z85_ALPHABET: bytes
    def a2b_base64(
        data: _AsciiBuffer,
        /,
        *,
        strict_mode: bool = False,
        alphabet: bytes = ...,
        padded: bool = True,
        ignorechars: ReadableBuffer = ...,
        canonical: bool = False,
    ) -> bytes: ...
    def b2a_base64(
        data: ReadableBuffer, /, *, newline: bool = True, alphabet: ReadableBuffer = ..., padded: bool = True, wrapcol: int = 0
    ) -> bytes: ...
    def b2a_base32(
        data: ReadableBuffer, /, *, alphabet: ReadableBuffer = ..., padded: bool = True, wrapcol: int = 0
    ) -> bytes: ...
    def a2b_base32(
        data: _AsciiBuffer,
        /,
        *,
        alphabet: bytes = ...,
        padded: bool = True,
        ignorechars: ReadableBuffer = b"",
        canonical: bool = False,
    ) -> bytes: ...
    def b2a_ascii85(
        data: ReadableBuffer, /, *, foldspaces: bool = False, wrapcol: int = 0, pad: bool = False, adobe: bool = False
    ) -> bytes: ...
    def a2b_ascii85(
        data: _AsciiBuffer,
        /,
        *,
        foldspaces: bool = False,
        adobe: bool = False,
        ignorechars: ReadableBuffer = b"",
        canonical: bool = False,
    ) -> bytes: ...
    def b2a_base85(data: ReadableBuffer, /, *, alphabet: ReadableBuffer = ..., pad: bool = False, wrapcol: int = 0) -> bytes: ...
    def a2b_base85(
        data: _AsciiBuffer, /, *, alphabet: bytes = ..., ignorechars: ReadableBuffer = b"", canonical: bool = False
    ) -> bytes: ...

elif sys.version_info >= (3, 11):
    def a2b_base64(data: _AsciiBuffer, /, *, strict_mode: bool = False) -> bytes: ...

else:
    def a2b_base64(data: _AsciiBuffer, /) -> bytes: ...

if sys.version_info < (3, 15):
    def b2a_base64(data: ReadableBuffer, /, *, newline: bool = True) -> bytes: ...

def a2b_qp(data: _AsciiBuffer, header: bool = False) -> bytes: ...
def b2a_qp(data: ReadableBuffer, quotetabs: bool = False, istext: bool = True, header: bool = False) -> bytes: ...

if sys.version_info < (3, 11):
    @deprecated("Deprecated since Python 3.9; removed in Python 3.11.")
    def a2b_hqx(data: _AsciiBuffer, /) -> bytes: ...
    @deprecated("Deprecated since Python 3.9; removed in Python 3.11.")
    def rledecode_hqx(data: ReadableBuffer, /) -> bytes: ...
    @deprecated("Deprecated since Python 3.9; removed in Python 3.11.")
    def rlecode_hqx(data: ReadableBuffer, /) -> bytes: ...
    @deprecated("Deprecated since Python 3.9; removed in Python 3.11.")
    def b2a_hqx(data: ReadableBuffer, /) -> bytes: ...

def crc_hqx(data: ReadableBuffer, crc: int, /) -> int: ...
def crc32(data: ReadableBuffer, crc: int = 0, /) -> int: ...
def b2a_hex(data: ReadableBuffer, sep: str | bytes = ..., bytes_per_sep: int = 1) -> bytes: ...
def hexlify(data: ReadableBuffer, sep: str | bytes = ..., bytes_per_sep: int = 1) -> bytes: ...

if sys.version_info >= (3, 15):
    def a2b_hex(hexstr: _AsciiBuffer, /, *, ignorechars: ReadableBuffer = b"") -> bytes: ...
    def unhexlify(hexstr: _AsciiBuffer, /, *, ignorechars: ReadableBuffer = b"") -> bytes: ...

else:
    def a2b_hex(hexstr: _AsciiBuffer, /) -> bytes: ...
    def unhexlify(hexstr: _AsciiBuffer, /) -> bytes: ...

class Error(ValueError): ...
class Incomplete(Exception): ...

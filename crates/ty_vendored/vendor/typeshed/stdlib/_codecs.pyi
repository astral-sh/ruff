import codecs
import sys
from _typeshed import ReadableBuffer
from collections.abc import Callable
from typing import Literal, final, overload, type_check_only
from typing_extensions import TypeAlias

# This type is not exposed; it is defined in unicodeobject.c
# At runtime it calls itself builtins.EncodingMap
@final
@type_check_only
class _EncodingMap:
    def size(self) -> int: ...

_CharMap: TypeAlias = dict[int, int] | _EncodingMap
_Handler: TypeAlias = Callable[[UnicodeError], tuple[str | bytes, int]]
_SearchFunction: TypeAlias = Callable[[str], codecs.CodecInfo | None]

def register(search_function: _SearchFunction, /) -> None:
    """Register a codec search function.

    Search functions are expected to take one argument, the encoding name in
    all lower case letters, and either return None, or a tuple of functions
    (encoder, decoder, stream_reader, stream_writer) (or a CodecInfo object).
    """

if sys.version_info >= (3, 10):
    def unregister(search_function: _SearchFunction, /) -> None:
        """Unregister a codec search function and clear the registry's cache.

        If the search function is not registered, do nothing.
        """

def register_error(errors: str, handler: _Handler, /) -> None:
    """Register the specified error handler under the name errors.

    handler must be a callable object, that will be called with an exception
    instance containing information about the location of the encoding/decoding
    error and must return a (replacement, new position) tuple.
    """

def lookup_error(name: str, /) -> _Handler:
    """lookup_error(errors) -> handler

    Return the error handler for the specified error handling name or raise a
    LookupError, if no handler exists under this name.
    """

# The type ignore on `encode` and `decode` is to avoid issues with overlapping overloads, for more details, see #300
# https://docs.python.org/3/library/codecs.html#binary-transforms
_BytesToBytesEncoding: TypeAlias = Literal[
    "base64",
    "base_64",
    "base64_codec",
    "bz2",
    "bz2_codec",
    "hex",
    "hex_codec",
    "quopri",
    "quotedprintable",
    "quoted_printable",
    "quopri_codec",
    "uu",
    "uu_codec",
    "zip",
    "zlib",
    "zlib_codec",
]
# https://docs.python.org/3/library/codecs.html#text-transforms
_StrToStrEncoding: TypeAlias = Literal["rot13", "rot_13"]

@overload
def encode(obj: ReadableBuffer, encoding: _BytesToBytesEncoding, errors: str = "strict") -> bytes:
    """Encodes obj using the codec registered for encoding.

    The default encoding is 'utf-8'.  errors may be given to set a
    different error handling scheme.  Default is 'strict' meaning that encoding
    errors raise a ValueError.  Other possible values are 'ignore', 'replace'
    and 'backslashreplace' as well as any other name registered with
    codecs.register_error that can handle ValueErrors.
    """

@overload
def encode(obj: str, encoding: _StrToStrEncoding, errors: str = "strict") -> str: ...  # type: ignore[overload-overlap]
@overload
def encode(obj: str, encoding: str = "utf-8", errors: str = "strict") -> bytes: ...
@overload
def decode(obj: ReadableBuffer, encoding: _BytesToBytesEncoding, errors: str = "strict") -> bytes:  # type: ignore[overload-overlap]
    """Decodes obj using the codec registered for encoding.

    Default encoding is 'utf-8'.  errors may be given to set a
    different error handling scheme.  Default is 'strict' meaning that encoding
    errors raise a ValueError.  Other possible values are 'ignore', 'replace'
    and 'backslashreplace' as well as any other name registered with
    codecs.register_error that can handle ValueErrors.
    """

@overload
def decode(obj: str, encoding: _StrToStrEncoding, errors: str = "strict") -> str: ...

# these are documented as text encodings but in practice they also accept str as input
@overload
def decode(
    obj: str,
    encoding: Literal["unicode_escape", "unicode-escape", "raw_unicode_escape", "raw-unicode-escape"],
    errors: str = "strict",
) -> str: ...

# hex is officially documented as a bytes to bytes encoding, but it appears to also work with str
@overload
def decode(obj: str, encoding: Literal["hex", "hex_codec"], errors: str = "strict") -> bytes: ...
@overload
def decode(obj: ReadableBuffer, encoding: str = "utf-8", errors: str = "strict") -> str: ...
def lookup(encoding: str, /) -> codecs.CodecInfo:
    """Looks up a codec tuple in the Python codec registry and returns a CodecInfo object."""

def charmap_build(map: str, /) -> _CharMap: ...
def ascii_decode(data: ReadableBuffer, errors: str | None = None, /) -> tuple[str, int]: ...
def ascii_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
def charmap_decode(data: ReadableBuffer, errors: str | None = None, mapping: _CharMap | None = None, /) -> tuple[str, int]: ...
def charmap_encode(str: str, errors: str | None = None, mapping: _CharMap | None = None, /) -> tuple[bytes, int]: ...
def escape_decode(data: str | ReadableBuffer, errors: str | None = None, /) -> tuple[str, int]: ...
def escape_encode(data: bytes, errors: str | None = None, /) -> tuple[bytes, int]: ...
def latin_1_decode(data: ReadableBuffer, errors: str | None = None, /) -> tuple[str, int]: ...
def latin_1_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
def raw_unicode_escape_decode(
    data: str | ReadableBuffer, errors: str | None = None, final: bool = True, /
) -> tuple[str, int]: ...
def raw_unicode_escape_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
def readbuffer_encode(data: str | ReadableBuffer, errors: str | None = None, /) -> tuple[bytes, int]: ...
def unicode_escape_decode(data: str | ReadableBuffer, errors: str | None = None, final: bool = True, /) -> tuple[str, int]: ...
def unicode_escape_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
def utf_16_be_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
def utf_16_be_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
def utf_16_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
def utf_16_encode(str: str, errors: str | None = None, byteorder: int = 0, /) -> tuple[bytes, int]: ...
def utf_16_ex_decode(
    data: ReadableBuffer, errors: str | None = None, byteorder: int = 0, final: bool = False, /
) -> tuple[str, int, int]: ...
def utf_16_le_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
def utf_16_le_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
def utf_32_be_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
def utf_32_be_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
def utf_32_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
def utf_32_encode(str: str, errors: str | None = None, byteorder: int = 0, /) -> tuple[bytes, int]: ...
def utf_32_ex_decode(
    data: ReadableBuffer, errors: str | None = None, byteorder: int = 0, final: bool = False, /
) -> tuple[str, int, int]: ...
def utf_32_le_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
def utf_32_le_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
def utf_7_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
def utf_7_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
def utf_8_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
def utf_8_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...

if sys.platform == "win32":
    def mbcs_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
    def mbcs_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
    def code_page_decode(
        codepage: int, data: ReadableBuffer, errors: str | None = None, final: bool = False, /
    ) -> tuple[str, int]: ...
    def code_page_encode(code_page: int, str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...
    def oem_decode(data: ReadableBuffer, errors: str | None = None, final: bool = False, /) -> tuple[str, int]: ...
    def oem_encode(str: str, errors: str | None = None, /) -> tuple[bytes, int]: ...

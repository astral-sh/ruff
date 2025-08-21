from collections.abc import Callable, Iterator
from email.message import Message
from typing import ClassVar, Final, overload

__all__ = ["Charset", "add_alias", "add_charset", "add_codec"]

QP: Final = 1  # undocumented
BASE64: Final = 2  # undocumented
SHORTEST: Final = 3  # undocumented
RFC2047_CHROME_LEN: Final = 7  # undocumented
DEFAULT_CHARSET: Final = "us-ascii"  # undocumented
UNKNOWN8BIT: Final = "unknown-8bit"  # undocumented
EMPTYSTRING: Final = ""  # undocumented
CHARSETS: Final[dict[str, tuple[int | None, int | None, str | None]]]
ALIASES: Final[dict[str, str]]
CODEC_MAP: Final[dict[str, str | None]]  # undocumented

class Charset:
    input_charset: str
    header_encoding: int
    body_encoding: int
    output_charset: str | None
    input_codec: str | None
    output_codec: str | None
    def __init__(self, input_charset: str = "us-ascii") -> None: ...
    def get_body_encoding(self) -> str | Callable[[Message], None]: ...
    def get_output_charset(self) -> str | None: ...
    def header_encode(self, string: str) -> str: ...
    def header_encode_lines(self, string: str, maxlengths: Iterator[int]) -> list[str | None]: ...
    @overload
    def body_encode(self, string: None) -> None: ...
    @overload
    def body_encode(self, string: str | bytes) -> str: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __eq__(self, other: object) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...

def add_charset(
    charset: str, header_enc: int | None = None, body_enc: int | None = None, output_charset: str | None = None
) -> None: ...
def add_alias(alias: str, canonical: str) -> None: ...
def add_codec(charset: str, codecname: str) -> None: ...

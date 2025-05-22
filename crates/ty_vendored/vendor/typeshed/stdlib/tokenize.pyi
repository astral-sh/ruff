import sys
from _typeshed import FileDescriptorOrPath
from collections.abc import Callable, Generator, Iterable, Sequence
from re import Pattern
from token import *
from token import EXACT_TOKEN_TYPES as EXACT_TOKEN_TYPES
from typing import Any, NamedTuple, TextIO, type_check_only
from typing_extensions import TypeAlias

__all__ = [
    "AMPER",
    "AMPEREQUAL",
    "AT",
    "ATEQUAL",
    "CIRCUMFLEX",
    "CIRCUMFLEXEQUAL",
    "COLON",
    "COLONEQUAL",
    "COMMA",
    "COMMENT",
    "DEDENT",
    "DOT",
    "DOUBLESLASH",
    "DOUBLESLASHEQUAL",
    "DOUBLESTAR",
    "DOUBLESTAREQUAL",
    "ELLIPSIS",
    "ENCODING",
    "ENDMARKER",
    "EQEQUAL",
    "EQUAL",
    "ERRORTOKEN",
    "GREATER",
    "GREATEREQUAL",
    "INDENT",
    "ISEOF",
    "ISNONTERMINAL",
    "ISTERMINAL",
    "LBRACE",
    "LEFTSHIFT",
    "LEFTSHIFTEQUAL",
    "LESS",
    "LESSEQUAL",
    "LPAR",
    "LSQB",
    "MINEQUAL",
    "MINUS",
    "NAME",
    "NEWLINE",
    "NL",
    "NOTEQUAL",
    "NT_OFFSET",
    "NUMBER",
    "N_TOKENS",
    "OP",
    "PERCENT",
    "PERCENTEQUAL",
    "PLUS",
    "PLUSEQUAL",
    "RARROW",
    "RBRACE",
    "RIGHTSHIFT",
    "RIGHTSHIFTEQUAL",
    "RPAR",
    "RSQB",
    "SEMI",
    "SLASH",
    "SLASHEQUAL",
    "STAR",
    "STAREQUAL",
    "STRING",
    "TILDE",
    "TYPE_COMMENT",
    "TYPE_IGNORE",
    "TokenInfo",
    "VBAR",
    "VBAREQUAL",
    "detect_encoding",
    "generate_tokens",
    "tok_name",
    "tokenize",
    "untokenize",
]
if sys.version_info < (3, 13):
    __all__ += ["ASYNC", "AWAIT"]

if sys.version_info >= (3, 10):
    __all__ += ["SOFT_KEYWORD"]

if sys.version_info >= (3, 12):
    __all__ += ["EXCLAMATION", "FSTRING_END", "FSTRING_MIDDLE", "FSTRING_START", "EXACT_TOKEN_TYPES"]

if sys.version_info >= (3, 13):
    __all__ += ["TokenError", "open"]

if sys.version_info >= (3, 14):
    __all__ += ["TSTRING_START", "TSTRING_MIDDLE", "TSTRING_END"]

cookie_re: Pattern[str]
blank_re: Pattern[bytes]

_Position: TypeAlias = tuple[int, int]

# This class is not exposed. It calls itself tokenize.TokenInfo.
@type_check_only
class _TokenInfo(NamedTuple):
    type: int
    string: str
    start: _Position
    end: _Position
    line: str

class TokenInfo(_TokenInfo):
    @property
    def exact_type(self) -> int: ...

# Backwards compatible tokens can be sequences of a shorter length too
_Token: TypeAlias = TokenInfo | Sequence[int | str | _Position]

class TokenError(Exception): ...

if sys.version_info < (3, 13):
    class StopTokenizing(Exception): ...  # undocumented

class Untokenizer:
    tokens: list[str]
    prev_row: int
    prev_col: int
    encoding: str | None
    def add_whitespace(self, start: _Position) -> None: ...
    if sys.version_info >= (3, 12):
        def add_backslash_continuation(self, start: _Position) -> None: ...

    def untokenize(self, iterable: Iterable[_Token]) -> str: ...
    def compat(self, token: Sequence[int | str], iterable: Iterable[_Token]) -> None: ...
    if sys.version_info >= (3, 12):
        def escape_brackets(self, token: str) -> str: ...

# Returns str, unless the ENCODING token is present, in which case it returns bytes.
def untokenize(iterable: Iterable[_Token]) -> str | Any: ...
def detect_encoding(readline: Callable[[], bytes | bytearray]) -> tuple[str, Sequence[bytes]]: ...
def tokenize(readline: Callable[[], bytes | bytearray]) -> Generator[TokenInfo, None, None]: ...
def generate_tokens(readline: Callable[[], str]) -> Generator[TokenInfo, None, None]: ...
def open(filename: FileDescriptorOrPath) -> TextIO: ...
def group(*choices: str) -> str: ...  # undocumented
def any(*choices: str) -> str: ...  # undocumented
def maybe(*choices: str) -> str: ...  # undocumented

Whitespace: str  # undocumented
Comment: str  # undocumented
Ignore: str  # undocumented
Name: str  # undocumented

Hexnumber: str  # undocumented
Binnumber: str  # undocumented
Octnumber: str  # undocumented
Decnumber: str  # undocumented
Intnumber: str  # undocumented
Exponent: str  # undocumented
Pointfloat: str  # undocumented
Expfloat: str  # undocumented
Floatnumber: str  # undocumented
Imagnumber: str  # undocumented
Number: str  # undocumented

def _all_string_prefixes() -> set[str]: ...  # undocumented

StringPrefix: str  # undocumented

Single: str  # undocumented
Double: str  # undocumented
Single3: str  # undocumented
Double3: str  # undocumented
Triple: str  # undocumented
String: str  # undocumented

Special: str  # undocumented
Funny: str  # undocumented

PlainToken: str  # undocumented
Token: str  # undocumented

ContStr: str  # undocumented
PseudoExtras: str  # undocumented
PseudoToken: str  # undocumented

endpats: dict[str, str]  # undocumented
single_quoted: set[str]  # undocumented
triple_quoted: set[str]  # undocumented

tabsize: int  # undocumented

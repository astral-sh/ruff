"""Tokenization help for Python programs.

tokenize(readline) is a generator that breaks a stream of bytes into
Python tokens.  It decodes the bytes according to PEP-0263 for
determining source file encoding.

It accepts a readline-like method which is called repeatedly to get the
next line of input (or b"" for EOF).  It generates 5-tuples with these
members:

    the token type (see token.py)
    the token (a string)
    the starting (row, column) indices of the token (a 2-tuple of ints)
    the ending (row, column) indices of the token (a 2-tuple of ints)
    the original line (string)

It is designed to match the working of the Python tokenizer exactly, except
that it produces COMMENT tokens for comments and gives type OP for all
operators.  Additionally, all token lists start with an ENCODING token
which tells you which encoding was used to decode the bytes stream.
"""

import sys
from _typeshed import FileDescriptorOrPath
from collections.abc import Callable, Generator, Iterable, Sequence
from re import Pattern
from token import *
from typing import Any, Final, NamedTuple, TextIO, type_check_only
from typing_extensions import TypeAlias, disjoint_base

if sys.version_info < (3, 12):
    # Avoid double assignment to Final name by imports, which pyright objects to.
    # EXACT_TOKEN_TYPES is already defined by 'from token import *' above
    # in Python 3.12+.
    from token import EXACT_TOKEN_TYPES as EXACT_TOKEN_TYPES

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

cookie_re: Final[Pattern[str]]
blank_re: Final[Pattern[bytes]]

_Position: TypeAlias = tuple[int, int]

# This class is not exposed. It calls itself tokenize.TokenInfo.
@type_check_only
class _TokenInfo(NamedTuple):
    type: int
    string: str
    start: _Position
    end: _Position
    line: str

if sys.version_info >= (3, 12):
    class TokenInfo(_TokenInfo):
        @property
        def exact_type(self) -> int: ...

else:
    @disjoint_base
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
        def add_backslash_continuation(self, start: _Position) -> None:
            """Add backslash continuation characters if the row has increased
            without encountering a newline token.

            This also inserts the correct amount of whitespace before the backslash.
            """

    def untokenize(self, iterable: Iterable[_Token]) -> str: ...
    def compat(self, token: Sequence[int | str], iterable: Iterable[_Token]) -> None: ...
    if sys.version_info >= (3, 12):
        def escape_brackets(self, token: str) -> str: ...

# Returns str, unless the ENCODING token is present, in which case it returns bytes.
def untokenize(iterable: Iterable[_Token]) -> str | Any:
    """Transform tokens back into Python source code.
    It returns a bytes object, encoded using the ENCODING
    token, which is the first token sequence output by tokenize.

    Each element returned by the iterable must be a token sequence
    with at least two elements, a token number and token value.  If
    only two tokens are passed, the resulting output is poor.

    The result is guaranteed to tokenize back to match the input so
    that the conversion is lossless and round-trips are assured.
    The guarantee applies only to the token type and token string as
    the spacing between tokens (column positions) may change.
    """

def detect_encoding(readline: Callable[[], bytes | bytearray]) -> tuple[str, Sequence[bytes]]:
    """
    The detect_encoding() function is used to detect the encoding that should
    be used to decode a Python source file.  It requires one argument, readline,
    in the same way as the tokenize() generator.

    It will call readline a maximum of twice, and return the encoding used
    (as a string) and a list of any lines (left as bytes) it has read in.

    It detects the encoding from the presence of a utf-8 bom or an encoding
    cookie as specified in pep-0263.  If both a bom and a cookie are present,
    but disagree, a SyntaxError will be raised.  If the encoding cookie is an
    invalid charset, raise a SyntaxError.  Note that if a utf-8 bom is found,
    'utf-8-sig' is returned.

    If no encoding is specified, then the default of 'utf-8' will be returned.
    """

def tokenize(readline: Callable[[], bytes | bytearray]) -> Generator[TokenInfo, None, None]:
    """
    The tokenize() generator requires one argument, readline, which
    must be a callable object which provides the same interface as the
    readline() method of built-in file objects.  Each call to the function
    should return one line of input as bytes.  Alternatively, readline
    can be a callable function terminating with StopIteration:
        readline = open(myfile, 'rb').__next__  # Example of alternate readline

    The generator produces 5-tuples with these members: the token type; the
    token string; a 2-tuple (srow, scol) of ints specifying the row and
    column where the token begins in the source; a 2-tuple (erow, ecol) of
    ints specifying the row and column where the token ends in the source;
    and the line on which the token was found.  The line passed is the
    physical line.

    The first token sequence will always be an ENCODING token
    which tells you which encoding was used to decode the bytes stream.
    """

def generate_tokens(readline: Callable[[], str]) -> Generator[TokenInfo, None, None]:
    """Tokenize a source reading Python code as unicode strings.

    This has the same API as tokenize(), except that it expects the *readline*
    callable to return str objects instead of bytes.
    """

def open(filename: FileDescriptorOrPath) -> TextIO:
    """Open a file in read only mode using the encoding detected by
    detect_encoding().
    """

def group(*choices: str) -> str: ...  # undocumented
def any(*choices: str) -> str: ...  # undocumented
def maybe(*choices: str) -> str: ...  # undocumented

Whitespace: Final[str]  # undocumented
Comment: Final[str]  # undocumented
Ignore: Final[str]  # undocumented
Name: Final[str]  # undocumented

Hexnumber: Final[str]  # undocumented
Binnumber: Final[str]  # undocumented
Octnumber: Final[str]  # undocumented
Decnumber: Final[str]  # undocumented
Intnumber: Final[str]  # undocumented
Exponent: Final[str]  # undocumented
Pointfloat: Final[str]  # undocumented
Expfloat: Final[str]  # undocumented
Floatnumber: Final[str]  # undocumented
Imagnumber: Final[str]  # undocumented
Number: Final[str]  # undocumented

def _all_string_prefixes() -> set[str]: ...  # undocumented

StringPrefix: Final[str]  # undocumented

Single: Final[str]  # undocumented
Double: Final[str]  # undocumented
Single3: Final[str]  # undocumented
Double3: Final[str]  # undocumented
Triple: Final[str]  # undocumented
String: Final[str]  # undocumented

Special: Final[str]  # undocumented
Funny: Final[str]  # undocumented

PlainToken: Final[str]  # undocumented
Token: Final[str]  # undocumented

ContStr: Final[str]  # undocumented
PseudoExtras: Final[str]  # undocumented
PseudoToken: Final[str]  # undocumented

endpats: Final[dict[str, str]]  # undocumented
single_quoted: Final[set[str]]  # undocumented
triple_quoted: Final[set[str]]  # undocumented

tabsize: Final = 8  # undocumented

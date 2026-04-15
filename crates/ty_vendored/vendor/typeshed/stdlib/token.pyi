import sys
from typing import Final

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
    "DEDENT",
    "DOT",
    "DOUBLESLASH",
    "DOUBLESLASHEQUAL",
    "DOUBLESTAR",
    "DOUBLESTAREQUAL",
    "ELLIPSIS",
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
    "VBAR",
    "VBAREQUAL",
    "tok_name",
    "ENCODING",
    "NL",
    "COMMENT",
]
if sys.version_info < (3, 13):
    __all__ += ["ASYNC", "AWAIT"]

if sys.version_info >= (3, 10):
    __all__ += ["SOFT_KEYWORD"]

if sys.version_info >= (3, 12):
    __all__ += ["EXCLAMATION", "FSTRING_END", "FSTRING_MIDDLE", "FSTRING_START", "EXACT_TOKEN_TYPES"]

if sys.version_info >= (3, 14):
    __all__ += ["TSTRING_START", "TSTRING_MIDDLE", "TSTRING_END"]

ENDMARKER: Final[int]
NAME: Final[int]
NUMBER: Final[int]
STRING: Final[int]
NEWLINE: Final[int]
INDENT: Final[int]
DEDENT: Final[int]
LPAR: Final[int]
RPAR: Final[int]
LSQB: Final[int]
RSQB: Final[int]
COLON: Final[int]
COMMA: Final[int]
SEMI: Final[int]
PLUS: Final[int]
MINUS: Final[int]
STAR: Final[int]
SLASH: Final[int]
VBAR: Final[int]
AMPER: Final[int]
LESS: Final[int]
GREATER: Final[int]
EQUAL: Final[int]
DOT: Final[int]
PERCENT: Final[int]
LBRACE: Final[int]
RBRACE: Final[int]
EQEQUAL: Final[int]
NOTEQUAL: Final[int]
LESSEQUAL: Final[int]
GREATEREQUAL: Final[int]
TILDE: Final[int]
CIRCUMFLEX: Final[int]
LEFTSHIFT: Final[int]
RIGHTSHIFT: Final[int]
DOUBLESTAR: Final[int]
PLUSEQUAL: Final[int]
MINEQUAL: Final[int]
STAREQUAL: Final[int]
SLASHEQUAL: Final[int]
PERCENTEQUAL: Final[int]
AMPEREQUAL: Final[int]
VBAREQUAL: Final[int]
CIRCUMFLEXEQUAL: Final[int]
LEFTSHIFTEQUAL: Final[int]
RIGHTSHIFTEQUAL: Final[int]
DOUBLESTAREQUAL: Final[int]
DOUBLESLASH: Final[int]
DOUBLESLASHEQUAL: Final[int]
AT: Final[int]
RARROW: Final[int]
ELLIPSIS: Final[int]
ATEQUAL: Final[int]
if sys.version_info < (3, 13):
    AWAIT: Final[int]
    ASYNC: Final[int]
OP: Final[int]
ERRORTOKEN: Final[int]
N_TOKENS: Final[int]
NT_OFFSET: Final[int]
tok_name: Final[dict[int, str]]
COMMENT: Final[int]
NL: Final[int]
ENCODING: Final[int]
TYPE_COMMENT: Final[int]
TYPE_IGNORE: Final[int]
COLONEQUAL: Final[int]
EXACT_TOKEN_TYPES: Final[dict[str, int]]
if sys.version_info >= (3, 10):
    SOFT_KEYWORD: Final[int]

if sys.version_info >= (3, 12):
    EXCLAMATION: Final[int]
    FSTRING_END: Final[int]
    FSTRING_MIDDLE: Final[int]
    FSTRING_START: Final[int]

if sys.version_info >= (3, 14):
    TSTRING_START: Final[int]
    TSTRING_MIDDLE: Final[int]
    TSTRING_END: Final[int]

def ISTERMINAL(x: int) -> bool: ...
def ISNONTERMINAL(x: int) -> bool: ...
def ISEOF(x: int) -> bool: ...

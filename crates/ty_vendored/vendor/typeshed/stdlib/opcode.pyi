import sys
from typing import Final, Literal

__all__ = [
    "cmp_op",
    "hasconst",
    "hasname",
    "hasjrel",
    "hasjabs",
    "haslocal",
    "hascompare",
    "hasfree",
    "opname",
    "opmap",
    "HAVE_ARGUMENT",
    "EXTENDED_ARG",
    "stack_effect",
]
if sys.version_info >= (3, 12):
    __all__ += ["hasarg", "hasexc"]
else:
    __all__ += ["hasnargs"]
if sys.version_info >= (3, 13):
    __all__ += ["hasjump"]

cmp_op: tuple[Literal["<"], Literal["<="], Literal["=="], Literal["!="], Literal[">"], Literal[">="]]
hasconst: Final[list[int]]
hasname: Final[list[int]]
hasjrel: Final[list[int]]
hasjabs: Final[list[int]]
haslocal: Final[list[int]]
hascompare: Final[list[int]]
hasfree: Final[list[int]]
if sys.version_info >= (3, 12):
    hasarg: Final[list[int]]
    hasexc: Final[list[int]]
else:
    hasnargs: Final[list[int]]
if sys.version_info >= (3, 13):
    hasjump: Final[list[int]]
opname: Final[list[str]]

opmap: Final[dict[str, int]]
HAVE_ARGUMENT: Final = 43
EXTENDED_ARG: Final = 69

def stack_effect(opcode: int, oparg: int | None = None, /, *, jump: bool | None = None) -> int: ...

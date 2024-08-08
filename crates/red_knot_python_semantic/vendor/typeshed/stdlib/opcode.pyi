import sys
from typing import Literal

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

if sys.version_info >= (3, 9):
    cmp_op: tuple[Literal["<"], Literal["<="], Literal["=="], Literal["!="], Literal[">"], Literal[">="]]
else:
    cmp_op: tuple[
        Literal["<"],
        Literal["<="],
        Literal["=="],
        Literal["!="],
        Literal[">"],
        Literal[">="],
        Literal["in"],
        Literal["not in"],
        Literal["is"],
        Literal["is not"],
        Literal["exception match"],
        Literal["BAD"],
    ]
hasconst: list[int]
hasname: list[int]
hasjrel: list[int]
hasjabs: list[int]
haslocal: list[int]
hascompare: list[int]
hasfree: list[int]
if sys.version_info >= (3, 12):
    hasarg: list[int]
    hasexc: list[int]
else:
    hasnargs: list[int]
if sys.version_info >= (3, 13):
    hasjump: list[int]
opname: list[str]

opmap: dict[str, int]
HAVE_ARGUMENT: int
EXTENDED_ARG: int

def stack_effect(opcode: int, oparg: int | None = None, /, *, jump: bool | None = None) -> int: ...

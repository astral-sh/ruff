# From https://github.com/PyCQA/flake8-pyi/blob/4212bec43dbc4020a59b90e2957c9488575e57ba/tests/type_comments.pyi

from collections.abc import Sequence
from typing import TypeAlias

A: TypeAlias = None  # type: int  # error
B: TypeAlias = None  # type: str  # And here's an extra comment about why it's that type  # error
C: TypeAlias = None  #type: int  # error
D: TypeAlias = None  #      type: int  # error
E: TypeAlias = None#    type: int  # error
F: TypeAlias = None#type:int  # error

def func(
    arg1,  # type: dict[str, int]  # error
    arg2  # type: Sequence[bytes]  # And here's some more info about this arg  # error
): ...

class Foo:
    Attr: TypeAlias = None  # type: set[str]  # error

G: TypeAlias = None  # type: ignore
H: TypeAlias = None  # type: ignore[attr-defined]
I: TypeAlias = None  #type: ignore
J: TypeAlias = None  #      type: ignore
K: TypeAlias = None#    type: ignore
L: TypeAlias = None#type:ignore

# Whole line commented out  # type: int
M: TypeAlias = None  # type: can't parse me!

class Bar:
    N: TypeAlias = None  # type: can't parse me either!
    # This whole line is commented out and indented # type: str

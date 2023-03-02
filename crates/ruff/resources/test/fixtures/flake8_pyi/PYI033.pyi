# From https://github.com/PyCQA/flake8-pyi/blob/4212bec43dbc4020a59b90e2957c9488575e57ba/tests/type_comments.pyi

from collections.abc import Sequence
from typing import TypeAlias

A: TypeAlias = None  # type: int  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")
B: TypeAlias = None  # type: str  # And here's an extra comment about why it's that type  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")
C: TypeAlias = None  #type: int  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")
D: TypeAlias = None  #      type: int  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")
E: TypeAlias = None#    type: int  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")
F: TypeAlias = None#type:int  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")

def func(
    arg1,  # type: dict[str, int]  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")
    arg2  # type: Sequence[bytes]  # And here's some more info about this arg  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")
): ...

class Foo:
    Attr: TypeAlias = None  # type: set[str]  # Y033 Do not use type comments in stubs (e.g. use "x: int" instead of "x = ... # type: int")

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

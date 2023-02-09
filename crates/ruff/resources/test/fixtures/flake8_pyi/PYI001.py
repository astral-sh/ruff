from typing import ParamSpec, TypeVar, TypeVarTuple

T = TypeVar("T")  # ok

TTuple = TypeVarTuple("TTuple") # ok

P = ParamSpec("P")  # ok


_T = TypeVar("_T")  # ok

_TTuple = TypeVarTuple("_TTuple") # ok

_P = ParamSpec("_P")  # ok

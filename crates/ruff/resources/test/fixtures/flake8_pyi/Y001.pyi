from typing import ParamSpec, TypeVar, TypeVarTuple

T = TypeVar("T")  # error, TypeVars in stubs must start with _

TTuple = TypeVarTuple("TTuple") # error, TypeVarTuples must also start with _

P = ParamSpec("P")  # error, ParamSpecs must start with _


_T = TypeVar("_T")  # ok

_TTuple = TypeVarTuple("_TTuple") # ok

_P = ParamSpec("_P")  # ok

from typing import ParamSpec, TypeVar, TypeVarTuple

T = TypeVar("T")  # Error: TypeVars in stubs must start with _

TTuple = TypeVarTuple("TTuple")  # Error: TypeVarTuples must also start with _

P = ParamSpec("P")  # Error: ParamSpecs must start with _

_T = TypeVar("_T")  # OK

_TTuple = TypeVarTuple("_TTuple")  # OK

_P = ParamSpec("_P")  # OK

def f():
    T = TypeVar("T")  # OK

from typing import ParamSpec, TypeVar, TypeVarTuple

T = TypeVar("T")  # OK

TTuple = TypeVarTuple("TTuple")  # OK

P = ParamSpec("P")  # OK

_T = TypeVar("_T")  # OK

_TTuple = TypeVarTuple("_TTuple")  # OK

_P = ParamSpec("_P")  # OK


def f():
    T = TypeVar("T")  # OK

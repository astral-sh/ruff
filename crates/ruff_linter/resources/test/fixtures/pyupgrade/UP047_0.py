from collections.abc import Callable
from typing import Any, AnyStr, ParamSpec, TypeVar, TypeVarTuple

from somewhere import Something

S = TypeVar("S", str, bytes)  # constrained type variable
T = TypeVar("T", bound=float)
Ts = TypeVarTuple("Ts")
P = ParamSpec("P")


def f(t: T) -> T:
    return t


def g(ts: tuple[*Ts]) -> tuple[*Ts]:
    return ts


def h(
    p: Callable[P, T],
    # Comment in the middle of a parameter list should be preserved
    another_param,
    and_another,
) -> Callable[P, T]:
    return p


def i(s: S) -> S:
    return s


# NOTE this case is the reason the fix is marked unsafe. If we can't confirm
# that one of the type parameters (`Something` in this case) is a TypeVar,
# which we can't do across module boundaries, we will not convert it to a
# generic type parameter. This leads to code that mixes old-style standalone
# TypeVars with the new-style generic syntax and will be rejected by type
# checkers
def broken_fix(okay: T, bad: Something) -> tuple[T, Something]:
    return (okay, bad)


def any_str_param(s: AnyStr) -> AnyStr:
    return s


# default requires 3.13
V = TypeVar("V", default=Any, bound=str)


def default_var(v: V) -> V:
    return v


# these cases are not handled

def outer():
    def inner(t: T) -> T:
        return t

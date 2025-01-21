from collections.abc import Callable
from typing import Any, ParamSpec, TypeVar, TypeVarTuple

from somewhere import Something

S = TypeVar("S", str, bytes)  # constrained type variable
T = TypeVar("T", bound=float)
Ts = TypeVarTuple("Ts")
P = ParamSpec("P")


def f(t: T):
    pass


def g(ts: tuple[*Ts]):
    pass


def h(
    p: Callable[P, T],
    # Comment in the middle of a parameter list should be preserved
    another_param,
    and_another,
):
    pass


def i(s: S):
    pass


# NOTE this case is the reason the fix is marked unsafe. If we can't confirm
# that one of the type parameters (`Something` in this case) is a TypeVar,
# which we can't do across module boundaries, we will not convert it to a
# generic type parameter. This leads to code that mixes old-style standalone
# TypeVars with the new-style generic syntax and will be rejected by type
# checkers
def broken_fix(okay: T, bad: Something):
    pass


# these cases are not handled

# TODO(brent) default requires 3.13
V = TypeVar("V", default=Any, bound=str)


def default_var(v: V):
    pass


def outer():
    def inner(t: T):
        pass

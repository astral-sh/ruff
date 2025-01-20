from collections.abc import Callable
from typing import Any, ParamSpec, TypeVar, TypeVarTuple

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


# these cases are not handled

# TODO(brent) default requires 3.13
V = TypeVar("V", default=Any, bound=str)


def default_var(v: V):
    pass


def outer():
    def inner(t: T):
        pass

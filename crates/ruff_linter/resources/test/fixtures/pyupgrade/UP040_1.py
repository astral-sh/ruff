from collections.abc import Callable
from typing import Generic, ParamSpec, TypeVar, TypeVarTuple

T = TypeVar("T", bound=float)
Ts = TypeVarTuple("Ts")
P = ParamSpec("P")


class A(Generic[T]):
    # Comments in a class body are preserved
    pass


class B(Generic[*Ts]):
    pass


class C(Generic[P]):
    pass


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

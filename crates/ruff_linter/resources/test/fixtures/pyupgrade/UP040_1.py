from typing import Generic, TypeVar, TypeVarTuple

T = TypeVar("T", bound=float)
Ts = TypeVarTuple("Ts")


class A(Generic[T]):
    pass


class B(Generic[*Ts]):
    pass


def f(t: T):
    pass

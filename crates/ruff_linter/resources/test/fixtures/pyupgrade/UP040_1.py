from typing import Generic, ParamSpec, TypeVar, TypeVarTuple

T = TypeVar("T", bound=float)
Ts = TypeVarTuple("Ts")
P = ParamSpec("P")


class A(Generic[T]):
    pass


class B(Generic[*Ts]):
    pass


class C(Generic[P]):
    pass


def f(t: T):
    pass

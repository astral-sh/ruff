from typing import Generic, TypeVar

T = TypeVar("T", bound=float)


class A(Generic[T]):
    pass


def f(t: T):
    pass

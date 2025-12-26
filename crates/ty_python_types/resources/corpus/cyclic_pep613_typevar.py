from typing import TypeAlias, TypeVar

T = TypeVar("T", bound="A[0]")
A: TypeAlias = T
def _(x: A):
    if x:
        pass

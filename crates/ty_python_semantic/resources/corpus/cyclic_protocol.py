# Regression test for https://github.com/astral-sh/ty/issues/3080

# To reproduce the bug, deferred evaluation of type annotations must be applied.
from __future__ import annotations

from typing import Generic, Protocol, Self, TypeVar, overload

S = TypeVar("S")
T = TypeVar("T")


class Unit(Protocol):
    def __mul__(self, other: S | Quantity[S]): ...


class Vector(Protocol): ...


class Quantity(Generic[T], Protocol):
    @overload
    def __mul__(self, other: Unit | Quantity[S]): ...

    @overload
    def __mul__(self, other: Vector) -> Vector: ...

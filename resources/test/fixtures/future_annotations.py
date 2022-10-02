from __future__ import annotations

from dataclasses import dataclass

from models import (
    Fruit,
    Nut,
)


@dataclass
class Foo:
    x: int
    y: int

    @classmethod
    def a(cls) -> Foo:
        return cls(x=0, y=0)

    @classmethod
    def b(cls) -> "Foo":
        return cls(x=0, y=0)

    @classmethod
    def c(cls) -> Bar:
        return cls(x=0, y=0)

    @classmethod
    def d(cls) -> Fruit:
        return cls(x=0, y=0)

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, List, Tuple, Optional, Sequence

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


def f(x: int) -> List[int]:
    y = List[int]()
    y.append(x)
    return y


x: Tuple[int, ...] = (1, 2)


def f(param: "Optional[Callable]" = None) -> "None":
    pass


def f(param: Optional["Sequence"] = None) -> "None":
    pass

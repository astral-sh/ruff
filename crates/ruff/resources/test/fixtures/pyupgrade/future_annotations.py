from __future__ import annotations

from dataclasses import dataclass
from typing import List, Optional, TypeAlias, Union

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


x: Optional[int] = None

MyList: TypeAlias = Union[List[int], List[str]]

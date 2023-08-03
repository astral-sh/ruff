import typing
from typing import TypeAlias

# UP100
x: typing.TypeAlias = int
x: TypeAlias = int


# UP100 with generics (todo)
T = typing.TypeVar["T"]
x: typing.TypeAlias = list[T]


# OK
x: TypeAlias
x: int = 1

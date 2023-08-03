import typing
from typing import TypeAlias

# UP040
x: typing.TypeAlias = int
x: TypeAlias = int


# UP040 with generics (todo)
T = typing.TypeVar["T"]
x: typing.TypeAlias = list[T]


# OK
x: TypeAlias
x: int = 1

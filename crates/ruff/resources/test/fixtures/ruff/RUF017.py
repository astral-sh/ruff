import typing
from typing import TypeAlias

# RUF017
x: typing.TypeAlias = int
x: TypeAlias = int


# RUF017 with generics (todo)
T = typing.TypeVar["T"]
x: typing.TypeAlias = list[T]


# OK
x: TypeAlias
x: int = 1

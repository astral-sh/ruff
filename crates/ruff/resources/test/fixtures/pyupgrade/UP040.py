import typing
from typing import TypeAlias

# UP040
x: typing.TypeAlias = int
x: TypeAlias = int


# UP040 simple generic
T = typing.TypeVar["T"]
x: typing.TypeAlias = list[T]


# UP040 bounded generic (todo)
T = typing.TypeVar("T", bound=int)
x: typing.TypeAlias = list[T]

T = typing.TypeVar("T", int, str)
x: typing.TypeAlias = list[T]

# UP040 contravariant generic (todo)
T = typing.TypeVar("T", contravariant=True)
x: typing.TypeAlias = list[T]

# UP040 covariant generic (todo)
T = typing.TypeVar("T", covariant=True)
x: typing.TypeAlias = list[T]

# OK
x: TypeAlias
x: int = 1

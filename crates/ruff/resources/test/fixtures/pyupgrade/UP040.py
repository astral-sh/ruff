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

# UP040 with function scope
def foo():
    TFUNC = typing.TypeVar("TFUNC")
    x: typing.TypeAlias = list[TFUNC]


# UP040 with class variable scope
class Foo:
    TCLS = typing.TypeVar("TCLS")
    x: typing.TypeAlias = list[TCLS]


# OK
x: TypeAlias
x: int = 1

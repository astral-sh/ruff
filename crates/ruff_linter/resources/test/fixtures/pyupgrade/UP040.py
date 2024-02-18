import typing
from typing import TypeAlias

# UP040
x: typing.TypeAlias = int
x: TypeAlias = int

# UP040 simple generic
T = typing.TypeVar["T"]
x: typing.TypeAlias = list[T]

# UP040 call style generic
T = typing.TypeVar("T")
x: typing.TypeAlias = list[T]

# UP040 bounded generic
T = typing.TypeVar("T", bound=int)
x: typing.TypeAlias = list[T]

# UP040 constrained generic
T = typing.TypeVar("T", int, str)
x: typing.TypeAlias = list[T]

# UP040 contravariant generic
T = typing.TypeVar("T", contravariant=True)
x: typing.TypeAlias = list[T]

# UP040 covariant generic
T = typing.TypeVar("T", covariant=True)
x: typing.TypeAlias = list[T]

# UP040 in class scope
T = typing.TypeVar["T"]
class Foo:
    # reference to global variable
    x: typing.TypeAlias = list[T]

    # reference to class variable
    TCLS = typing.TypeVar["TCLS"]
    y: typing.TypeAlias = list[TCLS]

# UP040 won't add generics in fix
T = typing.TypeVar(*args)
x: typing.TypeAlias = list[T]

# OK
x: TypeAlias
x: int = 1

# Ensure that "T" appears only once  in the type parameters for the modernized
# type alias.
T = typing.TypeVar["T"]
Decorator: TypeAlias = typing.Callable[[T], T]

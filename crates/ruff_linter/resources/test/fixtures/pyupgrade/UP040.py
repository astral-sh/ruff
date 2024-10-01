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


from typing import TypeVar, Annotated, TypeAliasType

from annotated_types import Gt, SupportGt


# https://github.com/astral-sh/ruff/issues/11422
T = TypeVar("T")
PositiveList = TypeAliasType(
    "PositiveList", list[Annotated[T, Gt(0)]], type_params=(T,)
)

# Bound
T = TypeVar("T", bound=SupportGt)
PositiveList = TypeAliasType(
    "PositiveList", list[Annotated[T, Gt(0)]], type_params=(T,)
)

# Multiple bounds
T1 = TypeVar("T1", bound=SupportGt)
T2 = TypeVar("T2")
T3 = TypeVar("T3")
Tuple3 = TypeAliasType("Tuple3", tuple[T1, T2, T3], type_params=(T1, T2, T3))

# No type_params
PositiveInt = TypeAliasType("PositiveInt", Annotated[int, Gt(0)])
PositiveInt = TypeAliasType("PositiveInt", Annotated[int, Gt(0)], type_params=())

# OK: Other name
T = TypeVar("T", bound=SupportGt)
PositiveList = TypeAliasType(
    "PositiveList2", list[Annotated[T, Gt(0)]], type_params=(T,)
)

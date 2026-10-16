import typing
from typing import TypeAlias

# UP040
# Fixes are unsafe even in stub files.
x: typing.TypeAlias = int
x: TypeAlias = int


# comments in the value are preserved
x: TypeAlias = tuple[
    int,  # preserved
    float,
]

T: TypeAlias = ( # comment0
    # comment1
    int  # comment2
    # comment3
    | # comment4
    # comment5
    str  # comment6
    # comment7
) # comment8

# Starred constraints remain safe in stub files, which are not executed.
constraints = (int, str)
T_starred = typing.TypeVar("T_starred", *constraints)
StarredList: TypeAlias = list[T_starred]
StarredAlias = typing.TypeAliasType("StarredAlias", list[T_starred], type_params=(T_starred,))

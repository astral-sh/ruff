import typing
from typing import TypeAlias

# UP040
# Fixes in type stub files should be safe to apply unlike in regular code where runtime behavior could change
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

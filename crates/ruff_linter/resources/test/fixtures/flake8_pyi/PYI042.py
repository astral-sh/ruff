import typing
from collections.abc import Mapping
from typing import (
    Annotated,
    TypeAlias,
    Union,
    Literal,
)

just_literals_pipe_union: TypeAlias = (
    Literal[True] | Literal["idk"]
)  # PYI042, since not camel case
PublicAliasT: TypeAlias = str | int
PublicAliasT2: TypeAlias = Union[str, bytes]
_ABCDEFGHIJKLMNOPQRST: TypeAlias = typing.Any
_PrivateAliasS: TypeAlias = Literal["I", "guess", "this", "is", "okay"]
_PrivateAliasS2: TypeAlias = Annotated[str, "also okay"]

snake_case_alias1: TypeAlias = str | int  # PYI042, since not camel case
_snake_case_alias2: TypeAlias = Literal["whatever"]  # PYI042, since not camel case
Snake_case_alias: TypeAlias = int | float  # PYI042, since not camel case

# check that this edge case doesn't crash
_: TypeAlias = str | int

# PEP 695
type foo_bar = int | str
type FooBar = int | str

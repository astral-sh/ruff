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
)  # not PYI042 (not a stubfile)
PublicAliasT: TypeAlias = str | int
PublicAliasT2: TypeAlias = Union[str, bytes]
_ABCDEFGHIJKLMNOPQRST: TypeAlias = typing.Any
_PrivateAliasS: TypeAlias = Literal["I", "guess", "this", "is", "okay"]
_PrivateAliasS2: TypeAlias = Annotated[str, "also okay"]

snake_case_alias1: TypeAlias = str | int  # not PYI042 (not a stubfile)
_snake_case_alias2: TypeAlias = Literal["whatever"]  # not PYI042 (not a stubfile)
Snake_case_alias: TypeAlias = int | float  # not PYI042 (not a stubfile)

# check that this edge case doesn't crash
_: TypeAlias = str | int

import typing
from collections.abc import Mapping
from typing import (
    Annotated,
    TypeAlias,
    Union,
    Literal,
)

_PrivateAliasT: TypeAlias = str | int  # not PYI043 (not a stubfile)
_PrivateAliasT2: TypeAlias = typing.Any  # not PYI043 (not a stubfile)
_PrivateAliasT3: TypeAlias = Literal[
    "not", "a", "chance"
]  # not PYI043 (not a stubfile)
just_literals_pipe_union: TypeAlias = Literal[True] | Literal["idk"]
PublicAliasT: TypeAlias = str | int
PublicAliasT2: TypeAlias = Union[str, bytes]
_ABCDEFGHIJKLMNOPQRST: TypeAlias = typing.Any
_PrivateAliasS: TypeAlias = Literal["I", "guess", "this", "is", "okay"]
_PrivateAliasS2: TypeAlias = Annotated[str, "also okay"]

# check that this edge case doesn't crash
_: TypeAlias = str | int

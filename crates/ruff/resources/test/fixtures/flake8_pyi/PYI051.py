import typing
from typing import Literal, TypeAlias, Union

one: str | Literal["foo"]
Two: TypeAlias = typing.Union[Literal[b"bar", b"baz"], bytes]

def func(x: str | Literal["bar"], y: Union[Literal[b"BAR", b"baz"], bytes]): ...

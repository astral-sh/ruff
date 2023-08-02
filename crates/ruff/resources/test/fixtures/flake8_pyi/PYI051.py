import typing
from typing import Literal, TypeAlias, Union

A: str | Literal["foo"]
B: TypeAlias = typing.Union[Literal[b"bar", b"foo"], bytes, str]
C: TypeAlias = typing.Union[Literal[5], int, typing.Union[Literal["foo"], str]]
D: TypeAlias = typing.Union[Literal[b"str_bytes", 42], bytes, int]

def func(x: complex | Literal[1J], y: Union[Literal[3.14], float]): ...

# OK
A: Literal["foo"]
B: TypeAlias = Literal[b"bar", b"foo"]
C: TypeAlias = typing.Union[Literal[5], Literal["foo"]]
D: TypeAlias = Literal[b"str_bytes", 42]

def func(x: Literal[1J], y: Literal[3.14]): ...

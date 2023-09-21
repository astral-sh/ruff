import typing
from typing import TypedDict


class _UnusedTypedDict(TypedDict):
    foo: str


class _UnusedTypedDict2(typing.TypedDict):
    bar: int


class _UsedTypedDict(TypedDict):
   foo: bytes


class _CustomClass(_UsedTypedDict):
    bar: list[int]

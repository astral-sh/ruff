import sys
import typing
from typing import TypedDict


class _UnusedTypedDict(TypedDict):
    foo: str


class _UnusedTypedDict2(typing.TypedDict):
    bar: int


# OK
class _UsedTypedDict(TypedDict):
   foo: bytes


class _CustomClass(_UsedTypedDict):
   bar: list[int]


if sys.version_info >= (3, 10):
   class _UsedTypedDict2(TypedDict):
       foo: int
else:
   class _UsedTypedDict2(TypedDict):
       foo: float


class _CustomClass2(_UsedTypedDict2):
   bar: list[int]

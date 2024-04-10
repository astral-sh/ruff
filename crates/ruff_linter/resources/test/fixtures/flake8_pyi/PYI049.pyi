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

_UnusedTypedDict3 = TypedDict("_UnusedTypedDict3", {"foo": int})
_UsedTypedDict3 = TypedDict("_UsedTypedDict3", {"bar": bytes})

def uses_UsedTypedDict3(arg: _UsedTypedDict3) -> None: ...


# In `.pyi` files, we flag unused definitions in class scopes as well as in the global
# scope (unlike in `.py` files).
class _CustomClass3:
    class _UnusedTypeDict4(TypedDict):
        pass

    def method(self) -> None:
        _CustomClass3._UnusedTypeDict4()

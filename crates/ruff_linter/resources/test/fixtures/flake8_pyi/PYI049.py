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


_UnusedTypedDict3 = TypedDict("_UnusedTypedDict3", {"foo": int})
_UsedTypedDict3 = TypedDict("_UsedTypedDict3", {"bar": bytes})


def uses_UsedTypedDict3(arg: _UsedTypedDict3) -> None: ...


# In `.py` files, we don't flag unused definitions in class scopes (unlike in `.pyi`
# files).
class _CustomClass3:
    class _UnusedTypeDict4(TypedDict):
        pass

    def method(self) -> None:
        _CustomClass3._UnusedTypeDict4()

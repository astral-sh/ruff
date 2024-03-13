import typing
from typing import Protocol, TypeVar


class _Foo(object, Protocol):
    bar: int


class _Bar(typing.Protocol):
    bar: int


_T = TypeVar("_T")


class _Baz(Protocol[_T]):
    x: _T


# OK
class _UsedPrivateProtocol(Protocol):
    bar: int


# Also OK
class _UsedGenericPrivateProtocol(Protocol[_T]):
    x: _T


def uses_some_private_protocols(
    arg: _UsedPrivateProtocol, arg2: _UsedGenericPrivateProtocol[int]
) -> None: ...

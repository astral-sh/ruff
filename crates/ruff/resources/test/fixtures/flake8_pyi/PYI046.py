import typing
from typing import Protocol


class _Foo(Protocol):
    bar: int


class _Bar(typing.Protocol):
    bar: int


# OK
class _UsedPrivateProtocol(Protocol):
    bar: int


def uses__UsedPrivateProtocol(arg: _UsedPrivateProtocol) -> None: ...

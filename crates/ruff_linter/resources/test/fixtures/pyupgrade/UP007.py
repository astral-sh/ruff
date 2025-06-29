import typing
from typing import NamedTuple, Optional, Union

import typing_extensions
from typing_extensions import Optional as OptionalTE


def f(x: Union[str, int, Union[float, bytes]]) -> None:
    ...


def f(x: typing.Union[str, int]) -> None:
    ...


def f(x: typing.Union[(str, int)]) -> None:
    ...


def f(x: typing.Union[(str, int), float]) -> None:
    ...


def f(x: typing.Union[(int,)]) -> None:
    ...


def f(x: typing.Union[()]) -> None:
    ...


def f(x: "Union[str, int, Union[float, bytes]]") -> None:
    ...


def f(x: "typing.Union[str, int]") -> None:
    ...


def f(x: Union["str", int]) -> None:
    ...


def f(x: Union[("str", "int"), float]) -> None:
    ...


def f() -> None:
    x = Union[str, int]
    x = Union["str", "int"]
    x: Union[str, int]
    x: Union["str", "int"]


def f(x: Union[int : float]) -> None:
    ...


def f(x: Union[str, int : float]) -> None:
    ...


def f(x: Union[x := int]) -> None:
    ...


def f(x: Union[str, x := int]) -> None:
    ...


def f(x: Union[lambda: int]) -> None:
    ...


def f(x: Union[str, lambda: int]) -> None:
    ...


# Regression test for: https://github.com/astral-sh/ruff/issues/7452
class Collection(Protocol[*_B0]):
    def __iter__(self) -> Iterator[Union[*_B0]]:
        ...


# Regression test for: https://github.com/astral-sh/ruff/issues/8609
def f(x: Union[int, str, bytes]) -> None:
    ...


# Regression test for https://github.com/astral-sh/ruff/issues/14132
class AClass:
    ...

def myfunc(param: "tuple[Union[int, 'AClass', None], str]"):
    print(param)


# Regression test for https://github.com/astral-sh/ruff/issues/18619
# Don't emit lint for `Optional[NamedTuple]`
a1: Optional[NamedTuple] = None
a2: typing.Optional[NamedTuple] = None
a3: OptionalTE[NamedTuple] = None
a4: typing_extensions.Optional[NamedTuple] = None
a5: Optional[typing.NamedTuple] = None
a6: typing.Optional[typing.NamedTuple] = None
a7: OptionalTE[typing.NamedTuple] = None
a8: typing_extensions.Optional[typing.NamedTuple] = None
a9: "Optional[NamedTuple]" = None

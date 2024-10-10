import typing
from typing import Union


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

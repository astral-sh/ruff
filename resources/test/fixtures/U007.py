import typing
from typing import Optional
from typing import Union


def f(x: Optional[str]) -> None:
    ...


def f(x: typing.Optional[str]) -> None:
    ...


def f(x: Union[str, int, Union[float, bytes]]) -> None:
    ...


def f(x: typing.Union[str, int]) -> None:
    ...


def f(x: typing.Union[(str, int)]) -> None:
    ...


def f(x: typing.Union[(str, int), float]) -> None:
    ...


def f(x: "Union[str, int, Union[float, bytes]]") -> None:
    ...


def f(x: "typing.Union[str, int]") -> None:
    ...


def f(x: Union["str", int]) -> None:
    ...


def f(x: Union[("str", "int"), float]) -> None:
    ...

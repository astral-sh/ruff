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
    x: Optional[str]
    x = Optional[str]

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


def f(x: Optional[int : float]) -> None:
    ...


def f(x: Optional[str, int : float]) -> None:
    ...


def f(x: Optional[int, float]) -> None:
    ...

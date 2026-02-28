import typing
from typing import Union


def f(x: Union[str, int, Union[float, bytes]]) -> None:
    ...


def f(x: typing.Union[str, int]) -> None:
    ...

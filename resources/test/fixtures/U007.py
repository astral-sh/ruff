from typing import Optional


def f(x: Optional[str]) -> None:
    ...


import typing


def f(x: typing.Optional[str]) -> None:
    ...


from typing import Union


def f(x: Union[str, int, Union[float, bytes]]) -> None:
    ...


import typing


def f(x: typing.Union[str, int]) -> None:
    ...


from typing import Union


def f(x: "Union[str, int, Union[float, bytes]]") -> None:
    ...


import typing


def f(x: "typing.Union[str, int]") -> None:
    ...

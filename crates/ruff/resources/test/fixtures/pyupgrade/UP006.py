import typing


def f(x: typing.List[str]) -> None:
    ...


from typing import List


def f(x: List[str]) -> None:
    ...


import typing as t


def f(x: t.List[str]) -> None:
    ...


from typing import List as IList


def f(x: IList[str]) -> None:
    ...


def f(x: "List[str]") -> None:
    ...

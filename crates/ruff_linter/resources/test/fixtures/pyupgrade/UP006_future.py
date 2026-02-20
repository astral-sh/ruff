from typing import List
import typing
import typing as t


def func1(a_list: List[str]) -> None:
    pass


def func2(a_list: typing.List[str]) -> None:
    pass


def func3(a_list: t.List[str]) -> None:
    pass


def func4(_: List[int]) -> None:
    a_list: t.List[str] = []

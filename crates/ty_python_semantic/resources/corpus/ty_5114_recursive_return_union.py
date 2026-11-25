# Regression test for https://github.com/astral-sh/ty/issues/5114

from typing import TypeVar, Union

T = TypeVar("T")
Tree = list[Union[list[list[T]], "Tree[T]"]]


def g(x: T) -> T | Tree[T]:
    return x


def f(x: T) -> T | Tree[T]:
    return g(x)

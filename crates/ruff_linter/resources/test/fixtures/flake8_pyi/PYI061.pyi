from typing import Literal


def func1(arg1: Literal[None]): ...


def func2(arg1: Literal[None] | int): ...


def func3() -> Literal[None]: ...


def func4(arg1: Literal[int, None, float]): ...


def func5(arg1: Literal[None, None]): ...


def func6(arg1: Literal[
    "hello",
    None  # Comment 1
    , "world"
]): ...


def func7(arg1: Literal[
    None  # Comment 1
]): ...


# OK
def good_func(arg1: Literal[int] | None): ...


# From flake8-pyi
Literal[None]  # PYI061 None inside "Literal[]" expression. Replace with "None"
Literal[True, None]  # PYI061 None inside "Literal[]" expression. Replace with "Literal[True] | None"

from typing import Never, NoReturn, Union

Union[Never, int]
Union[NoReturn, int]
Never | int
NoReturn | int
Union[Union[Never, int], Union[NoReturn, int]]
Union[NoReturn, int, float]


# Regression tests for https://github.com/astral-sh/ruff/issues/14567
x: None | Never | None
y: (None | Never) | None
z: None | (Never | None)

a: int | Never | None
b: Never | Never | None


def func() -> (
        Never  # text
        | # text
        int
): ...


# Regression test for https://github.com/astral-sh/ruff/issues/23207
# Multi-line single-remaining-element Union should be wrapped in parentheses
from typing import Literal

x: Union[
    Never,
    Literal["LongLiteralNumberOne"]
    | Literal["LongLiteralNumberTwo"]
    | Literal["LongLiteralNumberThree"]
]
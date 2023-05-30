"""Emit a warning when the ellipsis constant is used and can be avoided"""

from typing import List, overload, Union

# Ellipsis and preceding statement
try:
    A = 2
except ValueError:
    A = 24
    ...  # [unnecessary-ellipsis]


# Ellipsis in else statement
if True:
    A = 2
else:
    A = 24
    ...  # [unnecessary-ellipsis]


# Ellipsis in if and else statement
if True:
    A = 2
    ...  # [unnecessary-ellipsis]
else:
    A = 24
    ...  # [unnecessary-ellipsis]


def ellipsis_and_subsequent_statement():
    ...  # [unnecessary-ellipsis]
    return 0


# The parent of ellipsis is an assignment
B = ...
C = [..., 1, 2, 3]

# The parent of ellipsis is a call
if "X" is type(...):
    ...


def docstring_only():
    """In Python, stubbed functions often have a body that contains just a
    single `...` constant, indicating that the function doesn't do
    anything. However, a stubbed function can also have just a
    docstring, and function with a docstring and no body also does
    nothing.
    """


# This function has no docstring, so it needs a `...` constant.
def ellipsis_only():
    ...


def docstring_and_ellipsis():
    """This function doesn't do anything, but it has a docstring, so its
    `...` constant is useless clutter.

    NEW CHECK: unnecessary-ellipsis

    This would check for stubs with both docstrings and `...`
    constants, suggesting the removal of the useless `...`
    constants
    """
    ...  # [unnecessary-ellipsis]


class DocstringOnly:
    """The same goes for class stubs: docstring, or `...`, but not both."""


# No problem
class EllipsisOnly:
    ...


class MultipleEllipses:
    ...  # [unnecessary-ellipsis]
    ...
    ...  # [unnecessary-ellipsis]


class DocstringAndEllipsis:
    """Whoops! Mark this one as bad too."""

    ...  # [unnecessary-ellipsis]


# Function overloading
@overload
def summarize(data: int) -> float:
    ...


@overload
def summarize(data: str) -> str:
    ...


def summarize(data):
    if isinstance(data, str):
        ...
    return float(data)


# Method overloading
class MyIntegerList(List[int]):
    @overload
    def __getitem__(self, index: int) -> int:
        ...

    @overload
    def __getitem__(self, index: slice) -> List[int]:
        ...

    def __getitem__(self, index: Union[int, slice]) -> Union[int, List[int]]:
        if isinstance(index, int):
            ...
        elif isinstance(index, slice):
            ...
        else:
            raise TypeError(...)

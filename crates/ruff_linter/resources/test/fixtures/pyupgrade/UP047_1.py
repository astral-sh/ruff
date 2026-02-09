"""This is placed in a separate fixture as `TypeVar` needs to be imported
from `typing_extensions` to support default arguments in Python version < 3.13.
We verify that UP047 doesn't apply in this case.
"""

from typing_extensions import TypeVar

T = TypeVar("T", default=int)


def default_var(var: T) -> T:
    return var

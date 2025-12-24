"""This is placed in a separate fixture as `TypeVar` needs to be imported
from `typing_extensions` to support default arguments in Python version < 3.13.
We verify that UP046 doesn't apply in this case.
"""

from typing import Generic
from typing_extensions import TypeVar

T = TypeVar("T", default=str)


class DefaultTypeVar(Generic[T]):
    var: T

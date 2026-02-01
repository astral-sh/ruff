"""This is a template module just for instruction."""

import sys
from typing import Any, ClassVar, final

class Str(str): ...

@final
class Xxo:
    """A class that explicitly stores attributes in an internal dict"""

    def demo(self) -> None:
        """demo(o) -> o"""
    if sys.version_info >= (3, 11) and sys.platform != "win32":
        x_exports: int

def foo(i: int, j: int, /) -> Any:
    """foo(i,j)

    Return the sum of i and j.
    """

def new() -> Xxo:
    """new() -> new Xx object"""

if sys.version_info >= (3, 10):
    class Error(Exception): ...

else:
    class error(Exception): ...

    class Null:
        __hash__: ClassVar[None]  # type: ignore[assignment]

    def roj(b: Any, /) -> None:
        """roj(a,b) -> None"""

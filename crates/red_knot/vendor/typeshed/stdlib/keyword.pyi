import sys
from collections.abc import Sequence
from typing import Final

if sys.version_info >= (3, 9):
    __all__ = ["iskeyword", "issoftkeyword", "kwlist", "softkwlist"]
else:
    __all__ = ["iskeyword", "kwlist"]

def iskeyword(s: str, /) -> bool: ...

# a list at runtime, but you're not meant to mutate it;
# type it as a sequence
kwlist: Final[Sequence[str]]

if sys.version_info >= (3, 9):
    def issoftkeyword(s: str, /) -> bool: ...

    # a list at runtime, but you're not meant to mutate it;
    # type it as a sequence
    softkwlist: Final[Sequence[str]]

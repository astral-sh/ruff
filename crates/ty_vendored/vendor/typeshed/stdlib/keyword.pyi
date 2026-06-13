from collections.abc import Sequence
from typing import Final

__all__ = ["iskeyword", "issoftkeyword", "kwlist", "softkwlist"]

def iskeyword(s: str, /) -> bool: ...

# a list at runtime, but you're not meant to mutate it;
# type it as a sequence
kwlist: Final[Sequence[str]]

def issoftkeyword(s: str, /) -> bool: ...

# a list at runtime, but you're not meant to mutate it;
# type it as a sequence
softkwlist: Final[Sequence[str]]

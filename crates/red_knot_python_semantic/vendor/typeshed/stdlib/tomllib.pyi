from _typeshed import SupportsRead
from collections.abc import Callable
from typing import Any

__all__ = ("loads", "load", "TOMLDecodeError")

class TOMLDecodeError(ValueError): ...

def load(fp: SupportsRead[bytes], /, *, parse_float: Callable[[str], Any] = ...) -> dict[str, Any]: ...
def loads(s: str, /, *, parse_float: Callable[[str], Any] = ...) -> dict[str, Any]: ...

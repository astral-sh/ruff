import sys
from _typeshed import SupportsRead
from collections.abc import Callable
from typing import Any, overload
from typing_extensions import deprecated

__all__ = ("loads", "load", "TOMLDecodeError")

if sys.version_info >= (3, 14):
    class TOMLDecodeError(ValueError):
        msg: str
        doc: str
        pos: int
        lineno: int
        colno: int
        @overload
        def __init__(self, msg: str, doc: str, pos: int) -> None: ...
        @overload
        @deprecated("Deprecated in Python 3.14; Please set 'msg', 'doc' and 'pos' arguments only.")
        def __init__(self, msg: str | type = ..., doc: str | type = ..., pos: int | type = ..., *args: Any) -> None: ...

else:
    class TOMLDecodeError(ValueError): ...

def load(fp: SupportsRead[bytes], /, *, parse_float: Callable[[str], Any] = ...) -> dict[str, Any]: ...
def loads(s: str, /, *, parse_float: Callable[[str], Any] = ...) -> dict[str, Any]: ...

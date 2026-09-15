from collections.abc import Iterable, Sequence
from typing import Protocol, TypeVar, overload, type_check_only

_StrSequenceT_co = TypeVar("_StrSequenceT_co", covariant=True, bound=Sequence[str])

@type_check_only
class _SliceableT(Protocol[_StrSequenceT_co]):
    @overload
    def __getitem__(self, key: int, /) -> str: ...
    @overload
    def __getitem__(self, key: slice, /) -> _StrSequenceT_co: ...

__all__ = ["GetoptError", "error", "getopt", "gnu_getopt"]

def getopt(
    args: _SliceableT[_StrSequenceT_co], shortopts: str, longopts: Iterable[str] | str = []
) -> tuple[list[tuple[str, str]], _StrSequenceT_co]: ...
def gnu_getopt(
    args: Sequence[str], shortopts: str, longopts: Iterable[str] | str = []
) -> tuple[list[tuple[str, str]], list[str]]: ...

class GetoptError(Exception):
    msg: str
    opt: str
    def __init__(self, msg: str, opt: str = "") -> None: ...

error = GetoptError

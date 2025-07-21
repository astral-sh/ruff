from _typeshed import StrOrBytesPath, SupportsLenAndGetItem
from collections.abc import Iterable
from typing import Literal, TypeVar

_SourcesT = TypeVar("_SourcesT", bound=StrOrBytesPath)
_TargetsT = TypeVar("_TargetsT", bound=StrOrBytesPath)

def newer(source: StrOrBytesPath, target: StrOrBytesPath) -> bool | Literal[1]: ...
def newer_pairwise(
    sources: SupportsLenAndGetItem[_SourcesT], targets: SupportsLenAndGetItem[_TargetsT]
) -> tuple[list[_SourcesT], list[_TargetsT]]: ...
def newer_group(
    sources: Iterable[StrOrBytesPath], target: StrOrBytesPath, missing: Literal["error", "ignore", "newer"] = "error"
) -> Literal[0, 1]: ...

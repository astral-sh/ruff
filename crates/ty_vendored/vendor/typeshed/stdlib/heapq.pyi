import sys
from _heapq import *
from _typeshed import SupportsRichComparison
from collections.abc import Callable, Generator, Iterable
from typing import Any, Final, TypeVar

__all__ = ["heappush", "heappop", "heapify", "heapreplace", "merge", "nlargest", "nsmallest", "heappushpop"]

if sys.version_info >= (3, 14):
    # Added to __all__ in 3.14.1
    __all__ += ["heapify_max", "heappop_max", "heappush_max", "heappushpop_max", "heapreplace_max"]

_S = TypeVar("_S")

__about__: Final[str]

def merge(
    *iterables: Iterable[_S], key: Callable[[_S], SupportsRichComparison] | None = None, reverse: bool = False
) -> Generator[_S]: ...
def nlargest(n: int, iterable: Iterable[_S], key: Callable[[_S], SupportsRichComparison] | None = None) -> list[_S]: ...
def nsmallest(n: int, iterable: Iterable[_S], key: Callable[[_S], SupportsRichComparison] | None = None) -> list[_S]: ...
def _heapify_max(heap: list[Any], /) -> None: ...  # undocumented

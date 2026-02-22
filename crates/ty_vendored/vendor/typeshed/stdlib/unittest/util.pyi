"""Various utility functions."""

from collections.abc import MutableSequence, Sequence
from typing import Any, Final, Literal, Protocol, TypeVar, type_check_only
from typing_extensions import TypeAlias

@type_check_only
class _SupportsDunderLT(Protocol):
    def __lt__(self, other: Any, /) -> bool: ...

@type_check_only
class _SupportsDunderGT(Protocol):
    def __gt__(self, other: Any, /) -> bool: ...

@type_check_only
class _SupportsDunderLE(Protocol):
    def __le__(self, other: Any, /) -> bool: ...

@type_check_only
class _SupportsDunderGE(Protocol):
    def __ge__(self, other: Any, /) -> bool: ...

_T = TypeVar("_T")
_Mismatch: TypeAlias = tuple[_T, _T, int]
_SupportsComparison: TypeAlias = _SupportsDunderLE | _SupportsDunderGE | _SupportsDunderGT | _SupportsDunderLT

_MAX_LENGTH: Final = 80
_PLACEHOLDER_LEN: Final = 12
_MIN_BEGIN_LEN: Final = 5
_MIN_END_LEN: Final = 5
_MIN_COMMON_LEN: Final = 5
_MIN_DIFF_LEN: Final = 41

def _shorten(s: str, prefixlen: int, suffixlen: int) -> str: ...
def _common_shorten_repr(*args: str) -> tuple[str, ...]: ...
def safe_repr(obj: object, short: bool = False) -> str: ...
def strclass(cls: type) -> str: ...
def sorted_list_difference(expected: Sequence[_T], actual: Sequence[_T]) -> tuple[list[_T], list[_T]]:
    """Finds elements in only one or the other of two, sorted input lists.

    Returns a two-element tuple of lists.    The first list contains those
    elements in the "expected" list but not in the "actual" list, and the
    second contains those elements in the "actual" list but not in the
    "expected" list.    Duplicate elements in either input list are ignored.
    """

def unorderable_list_difference(expected: MutableSequence[_T], actual: MutableSequence[_T]) -> tuple[list[_T], list[_T]]:
    """Same behavior as sorted_list_difference but
    for lists of unorderable items (like dicts).

    As it does a linear search per item (remove) it
    has O(n*n) performance.
    """

def three_way_cmp(x: _SupportsComparison, y: _SupportsComparison) -> Literal[-1, 0, 1]:
    """Return -1 if x < y, 0 if x == y and 1 if x > y"""

def _count_diff_all_purpose(actual: Sequence[_T], expected: Sequence[_T]) -> list[_Mismatch[_T]]:
    """Returns list of (cnt_act, cnt_exp, elem) triples where the counts differ"""

def _count_diff_hashable(actual: Sequence[_T], expected: Sequence[_T]) -> list[_Mismatch[_T]]:
    """Returns list of (cnt_act, cnt_exp, elem) triples where the counts differ"""

"""Various utility functions."""

from collections.abc import MutableSequence, Sequence
from typing import Any, Final, TypeVar
from typing_extensions import TypeAlias

_T = TypeVar("_T")
_Mismatch: TypeAlias = tuple[_T, _T, int]

_MAX_LENGTH: Final[int]
_PLACEHOLDER_LEN: Final[int]
_MIN_BEGIN_LEN: Final[int]
_MIN_END_LEN: Final[int]
_MIN_COMMON_LEN: Final[int]
_MIN_DIFF_LEN: Final[int]

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

def three_way_cmp(x: Any, y: Any) -> int:
    """Return -1 if x < y, 0 if x == y and 1 if x > y"""

def _count_diff_all_purpose(actual: Sequence[_T], expected: Sequence[_T]) -> list[_Mismatch[_T]]:
    """Returns list of (cnt_act, cnt_exp, elem) triples where the counts differ"""

def _count_diff_hashable(actual: Sequence[_T], expected: Sequence[_T]) -> list[_Mismatch[_T]]:
    """Returns list of (cnt_act, cnt_exp, elem) triples where the counts differ"""

"""Bisection algorithms.

This module provides support for maintaining a list in sorted order without
having to sort the list after each insertion. For long lists of items with
expensive comparison operations, this can be an improvement over the more
common approach.
"""

import sys
from _typeshed import SupportsLenAndGetItem, SupportsRichComparisonT
from collections.abc import Callable, MutableSequence
from typing import TypeVar, overload

_T = TypeVar("_T")

if sys.version_info >= (3, 10):
    @overload
    def bisect_left(
        a: SupportsLenAndGetItem[SupportsRichComparisonT],
        x: SupportsRichComparisonT,
        lo: int = 0,
        hi: int | None = None,
        *,
        key: None = None,
    ) -> int:
        """Return the index where to insert item x in list a, assuming a is sorted.

        The return value i is such that all e in a[:i] have e < x, and all e in
        a[i:] have e >= x.  So if x already appears in the list, a.insert(i, x) will
        insert just before the leftmost x already there.

        Optional args lo (default 0) and hi (default len(a)) bound the
        slice of a to be searched.

        A custom key function can be supplied to customize the sort order.
        """

    @overload
    def bisect_left(
        a: SupportsLenAndGetItem[_T],
        x: SupportsRichComparisonT,
        lo: int = 0,
        hi: int | None = None,
        *,
        key: Callable[[_T], SupportsRichComparisonT],
    ) -> int: ...
    @overload
    def bisect_right(
        a: SupportsLenAndGetItem[SupportsRichComparisonT],
        x: SupportsRichComparisonT,
        lo: int = 0,
        hi: int | None = None,
        *,
        key: None = None,
    ) -> int:
        """Return the index where to insert item x in list a, assuming a is sorted.

        The return value i is such that all e in a[:i] have e <= x, and all e in
        a[i:] have e > x.  So if x already appears in the list, a.insert(i, x) will
        insert just after the rightmost x already there.

        Optional args lo (default 0) and hi (default len(a)) bound the
        slice of a to be searched.

        A custom key function can be supplied to customize the sort order.
        """

    @overload
    def bisect_right(
        a: SupportsLenAndGetItem[_T],
        x: SupportsRichComparisonT,
        lo: int = 0,
        hi: int | None = None,
        *,
        key: Callable[[_T], SupportsRichComparisonT],
    ) -> int: ...
    @overload
    def insort_left(
        a: MutableSequence[SupportsRichComparisonT],
        x: SupportsRichComparisonT,
        lo: int = 0,
        hi: int | None = None,
        *,
        key: None = None,
    ) -> None:
        """Insert item x in list a, and keep it sorted assuming a is sorted.

        If x is already in a, insert it to the left of the leftmost x.

        Optional args lo (default 0) and hi (default len(a)) bound the
        slice of a to be searched.

        A custom key function can be supplied to customize the sort order.
        """

    @overload
    def insort_left(
        a: MutableSequence[_T], x: _T, lo: int = 0, hi: int | None = None, *, key: Callable[[_T], SupportsRichComparisonT]
    ) -> None: ...
    @overload
    def insort_right(
        a: MutableSequence[SupportsRichComparisonT],
        x: SupportsRichComparisonT,
        lo: int = 0,
        hi: int | None = None,
        *,
        key: None = None,
    ) -> None:
        """Insert item x in list a, and keep it sorted assuming a is sorted.

        If x is already in a, insert it to the right of the rightmost x.

        Optional args lo (default 0) and hi (default len(a)) bound the
        slice of a to be searched.

        A custom key function can be supplied to customize the sort order.
        """

    @overload
    def insort_right(
        a: MutableSequence[_T], x: _T, lo: int = 0, hi: int | None = None, *, key: Callable[[_T], SupportsRichComparisonT]
    ) -> None: ...

else:
    def bisect_left(
        a: SupportsLenAndGetItem[SupportsRichComparisonT], x: SupportsRichComparisonT, lo: int = 0, hi: int | None = None
    ) -> int:
        """Return the index where to insert item x in list a, assuming a is sorted.

        The return value i is such that all e in a[:i] have e < x, and all e in
        a[i:] have e >= x.  So if x already appears in the list, i points just
        before the leftmost x already there.

        Optional args lo (default 0) and hi (default len(a)) bound the
        slice of a to be searched.
        """

    def bisect_right(
        a: SupportsLenAndGetItem[SupportsRichComparisonT], x: SupportsRichComparisonT, lo: int = 0, hi: int | None = None
    ) -> int:
        """Return the index where to insert item x in list a, assuming a is sorted.

        The return value i is such that all e in a[:i] have e <= x, and all e in
        a[i:] have e > x.  So if x already appears in the list, i points just
        beyond the rightmost x already there

        Optional args lo (default 0) and hi (default len(a)) bound the
        slice of a to be searched.
        """

    def insort_left(
        a: MutableSequence[SupportsRichComparisonT], x: SupportsRichComparisonT, lo: int = 0, hi: int | None = None
    ) -> None:
        """Insert item x in list a, and keep it sorted assuming a is sorted.

        If x is already in a, insert it to the left of the leftmost x.

        Optional args lo (default 0) and hi (default len(a)) bound the
        slice of a to be searched.
        """

    def insort_right(
        a: MutableSequence[SupportsRichComparisonT], x: SupportsRichComparisonT, lo: int = 0, hi: int | None = None
    ) -> None:
        """Insert item x in list a, and keep it sorted assuming a is sorted.

        If x is already in a, insert it to the right of the rightmost x.

        Optional args lo (default 0) and hi (default len(a)) bound the
        slice of a to be searched.
        """

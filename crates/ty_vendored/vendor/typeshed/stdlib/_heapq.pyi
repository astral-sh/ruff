"""Heap queue algorithm (a.k.a. priority queue).

Heaps are arrays for which a[k] <= a[2*k+1] and a[k] <= a[2*k+2] for
all k, counting elements from 0.  For the sake of comparison,
non-existing elements are considered to be infinite.  The interesting
property of a heap is that a[0] is always its smallest element.

Usage:

heap = []            # creates an empty heap
heappush(heap, item) # pushes a new item on the heap
item = heappop(heap) # pops the smallest item from the heap
item = heap[0]       # smallest item on the heap without popping it
heapify(x)           # transforms list into a heap, in-place, in linear time
item = heapreplace(heap, item) # pops and returns smallest item, and adds
                               # new item; the heap size is unchanged

Our API differs from textbook heap algorithms as follows:

- We use 0-based indexing.  This makes the relationship between the
  index for a node and the indexes for its children slightly less
  obvious, but is more suitable since Python uses 0-based indexing.

- Our heappop() method returns the smallest item, not the largest.

These two make it possible to view the heap as a regular Python list
without surprises: heap[0] is the smallest item, and heap.sort()
maintains the heap invariant!
"""

import sys
from _typeshed import SupportsRichComparisonT as _T  # All type variable use in this module requires comparability.
from typing import Final

__about__: Final[str]

def heapify(heap: list[_T], /) -> None:
    """Transform list into a heap, in-place, in O(len(heap)) time."""

def heappop(heap: list[_T], /) -> _T:
    """Pop the smallest item off the heap, maintaining the heap invariant."""

def heappush(heap: list[_T], item: _T, /) -> None:
    """Push item onto heap, maintaining the heap invariant."""

def heappushpop(heap: list[_T], item: _T, /) -> _T:
    """Push item on the heap, then pop and return the smallest item from the heap.

    The combined action runs more efficiently than heappush() followed by
    a separate call to heappop().
    """

def heapreplace(heap: list[_T], item: _T, /) -> _T:
    """Pop and return the current smallest value, and add the new item.

    This is more efficient than heappop() followed by heappush(), and can be
    more appropriate when using a fixed-size heap.  Note that the value
    returned may be larger than item!  That constrains reasonable uses of
    this routine unless written as part of a conditional replacement:

        if item > heap[0]:
            item = heapreplace(heap, item)
    """

if sys.version_info >= (3, 14):
    def heapify_max(heap: list[_T], /) -> None:
        """Maxheap variant of heapify."""

    def heappop_max(heap: list[_T], /) -> _T:
        """Maxheap variant of heappop."""

    def heappush_max(heap: list[_T], item: _T, /) -> None:
        """Push item onto max heap, maintaining the heap invariant."""

    def heappushpop_max(heap: list[_T], item: _T, /) -> _T:
        """Maxheap variant of heappushpop.

        The combined action runs more efficiently than heappush_max() followed by
        a separate call to heappop_max().
        """

    def heapreplace_max(heap: list[_T], item: _T, /) -> _T:
        """Maxheap variant of heapreplace."""

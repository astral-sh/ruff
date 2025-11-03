"""Functional tools for creating and using iterators.

Infinite iterators:
count(start=0, step=1) --> start, start+step, start+2*step, ...
cycle(p) --> p0, p1, ... plast, p0, p1, ...
repeat(elem [,n]) --> elem, elem, elem, ... endlessly or up to n times

Iterators terminating on the shortest input sequence:
accumulate(p[, func]) --> p0, p0+p1, p0+p1+p2
batched(p, n) --> [p0, p1, ..., p_n-1], [p_n, p_n+1, ..., p_2n-1], ...
chain(p, q, ...) --> p0, p1, ... plast, q0, q1, ...
chain.from_iterable([p, q, ...]) --> p0, p1, ... plast, q0, q1, ...
compress(data, selectors) --> (d[0] if s[0]), (d[1] if s[1]), ...
dropwhile(predicate, seq) --> seq[n], seq[n+1], starting when predicate fails
groupby(iterable[, keyfunc]) --> sub-iterators grouped by value of keyfunc(v)
filterfalse(predicate, seq) --> elements of seq where predicate(elem) is False
islice(seq, [start,] stop [, step]) --> elements from
       seq[start:stop:step]
pairwise(s) --> (s[0],s[1]), (s[1],s[2]), (s[2], s[3]), ...
starmap(fun, seq) --> fun(*seq[0]), fun(*seq[1]), ...
tee(it, n=2) --> (it1, it2 , ... itn) splits one iterator into n
takewhile(predicate, seq) --> seq[0], seq[1], until predicate fails
zip_longest(p, q, ...) --> (p[0], q[0]), (p[1], q[1]), ...

Combinatoric generators:
product(p, q, ... [repeat=1]) --> cartesian product
permutations(p[, r])
combinations(p, r)
combinations_with_replacement(p, r)
"""

import sys
from _typeshed import MaybeNone
from collections.abc import Callable, Iterable, Iterator
from types import GenericAlias
from typing import Any, Generic, Literal, SupportsComplex, SupportsFloat, SupportsIndex, SupportsInt, TypeVar, overload
from typing_extensions import Self, TypeAlias, disjoint_base

_T = TypeVar("_T")
_S = TypeVar("_S")
_N = TypeVar("_N", int, float, SupportsFloat, SupportsInt, SupportsIndex, SupportsComplex)
_T_co = TypeVar("_T_co", covariant=True)
_S_co = TypeVar("_S_co", covariant=True)
_T1 = TypeVar("_T1")
_T2 = TypeVar("_T2")
_T3 = TypeVar("_T3")
_T4 = TypeVar("_T4")
_T5 = TypeVar("_T5")
_T6 = TypeVar("_T6")
_T7 = TypeVar("_T7")
_T8 = TypeVar("_T8")
_T9 = TypeVar("_T9")
_T10 = TypeVar("_T10")

_Step: TypeAlias = SupportsFloat | SupportsInt | SupportsIndex | SupportsComplex

_Predicate: TypeAlias = Callable[[_T], object]

# Technically count can take anything that implements a number protocol and has an add method
# but we can't enforce the add method
@disjoint_base
class count(Generic[_N]):
    """Return a count object whose .__next__() method returns consecutive values.

    Equivalent to:
        def count(firstval=0, step=1):
            x = firstval
            while 1:
                yield x
                x += step
    """

    @overload
    def __new__(cls) -> count[int]: ...
    @overload
    def __new__(cls, start: _N, step: _Step = ...) -> count[_N]: ...
    @overload
    def __new__(cls, *, step: _N) -> count[_N]: ...
    def __next__(self) -> _N:
        """Implement next(self)."""

    def __iter__(self) -> Self:
        """Implement iter(self)."""

@disjoint_base
class cycle(Generic[_T]):
    """Return elements from the iterable until it is exhausted. Then repeat the sequence indefinitely."""

    def __new__(cls, iterable: Iterable[_T], /) -> Self: ...
    def __next__(self) -> _T:
        """Implement next(self)."""

    def __iter__(self) -> Self:
        """Implement iter(self)."""

@disjoint_base
class repeat(Generic[_T]):
    """repeat(object [,times]) -> create an iterator which returns the object
    for the specified number of times.  If not specified, returns the object
    endlessly.
    """

    @overload
    def __new__(cls, object: _T) -> Self: ...
    @overload
    def __new__(cls, object: _T, times: int) -> Self: ...
    def __next__(self) -> _T:
        """Implement next(self)."""

    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __length_hint__(self) -> int:
        """Private method returning an estimate of len(list(it))."""

@disjoint_base
class accumulate(Generic[_T]):
    """Return series of accumulated sums (or other binary function results)."""

    @overload
    def __new__(cls, iterable: Iterable[_T], func: None = None, *, initial: _T | None = ...) -> Self: ...
    @overload
    def __new__(cls, iterable: Iterable[_S], func: Callable[[_T, _S], _T], *, initial: _T | None = ...) -> Self: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T:
        """Implement next(self)."""

@disjoint_base
class chain(Generic[_T]):
    """Return a chain object whose .__next__() method returns elements from the
    first iterable until it is exhausted, then elements from the next
    iterable, until all of the iterables are exhausted.
    """

    def __new__(cls, *iterables: Iterable[_T]) -> Self: ...
    def __next__(self) -> _T:
        """Implement next(self)."""

    def __iter__(self) -> Self:
        """Implement iter(self)."""

    @classmethod
    # We use type[Any] and not type[_S] to not lose the type inference from __iterable
    def from_iterable(cls: type[Any], iterable: Iterable[Iterable[_S]], /) -> chain[_S]:
        """Alternative chain() constructor taking a single iterable argument that evaluates lazily."""

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

@disjoint_base
class compress(Generic[_T]):
    """Return data elements corresponding to true selector elements.

    Forms a shorter iterator from selected data elements using the selectors to
    choose the data elements.
    """

    def __new__(cls, data: Iterable[_T], selectors: Iterable[Any]) -> Self: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T:
        """Implement next(self)."""

@disjoint_base
class dropwhile(Generic[_T]):
    """Drop items from the iterable while predicate(item) is true.

    Afterwards, return every element until the iterable is exhausted.
    """

    def __new__(cls, predicate: _Predicate[_T], iterable: Iterable[_T], /) -> Self: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T:
        """Implement next(self)."""

@disjoint_base
class filterfalse(Generic[_T]):
    """Return those items of iterable for which function(item) is false.

    If function is None, return the items that are false.
    """

    def __new__(cls, function: _Predicate[_T] | None, iterable: Iterable[_T], /) -> Self: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T:
        """Implement next(self)."""

@disjoint_base
class groupby(Generic[_T_co, _S_co]):
    """make an iterator that returns consecutive keys and groups from the iterable

    iterable
      Elements to divide into groups according to the key function.
    key
      A function for computing the group category for each element.
      If the key function is not specified or is None, the element itself
      is used for grouping.
    """

    @overload
    def __new__(cls, iterable: Iterable[_T1], key: None = None) -> groupby[_T1, _T1]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T1], key: Callable[[_T1], _T2]) -> groupby[_T2, _T1]: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> tuple[_T_co, Iterator[_S_co]]:
        """Implement next(self)."""

@disjoint_base
class islice(Generic[_T]):
    """islice(iterable, stop) --> islice object
    islice(iterable, start, stop[, step]) --> islice object

    Return an iterator whose next() method returns selected values from an
    iterable.  If start is specified, will skip all preceding elements;
    otherwise, start defaults to zero.  Step defaults to one.  If
    specified as another value, step determines how many values are
    skipped between successive calls.  Works like a slice() on a list
    but returns an iterator.
    """

    @overload
    def __new__(cls, iterable: Iterable[_T], stop: int | None, /) -> Self: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], start: int | None, stop: int | None, step: int | None = ..., /) -> Self: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T:
        """Implement next(self)."""

@disjoint_base
class starmap(Generic[_T_co]):
    """Return an iterator whose values are returned from the function evaluated with an argument tuple taken from the given sequence."""

    def __new__(cls, function: Callable[..., _T], iterable: Iterable[Iterable[Any]], /) -> starmap[_T]: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T_co:
        """Implement next(self)."""

@disjoint_base
class takewhile(Generic[_T]):
    """Return successive entries from an iterable as long as the predicate evaluates to true for each entry."""

    def __new__(cls, predicate: _Predicate[_T], iterable: Iterable[_T], /) -> Self: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T:
        """Implement next(self)."""

def tee(iterable: Iterable[_T], n: int = 2, /) -> tuple[Iterator[_T], ...]:
    """Returns a tuple of n independent iterators."""

@disjoint_base
class zip_longest(Generic[_T_co]):
    """Return a zip_longest object whose .__next__() method returns a tuple where
    the i-th element comes from the i-th iterable argument.  The .__next__()
    method continues until the longest iterable in the argument sequence
    is exhausted and then it raises StopIteration.  When the shorter iterables
    are exhausted, the fillvalue is substituted in their place.  The fillvalue
    defaults to None or can be specified by a keyword argument.
    """

    # one iterable (fillvalue doesn't matter)
    @overload
    def __new__(cls, iter1: Iterable[_T1], /, *, fillvalue: object = ...) -> zip_longest[tuple[_T1]]: ...
    # two iterables
    @overload
    # In the overloads without fillvalue, all of the tuple members could theoretically be None,
    # but we return Any instead to avoid false positives for code where we know one of the iterables
    # is longer.
    def __new__(cls, iter1: Iterable[_T1], iter2: Iterable[_T2], /) -> zip_longest[tuple[_T1 | MaybeNone, _T2 | MaybeNone]]: ...
    @overload
    def __new__(
        cls, iter1: Iterable[_T1], iter2: Iterable[_T2], /, *, fillvalue: _T
    ) -> zip_longest[tuple[_T1 | _T, _T2 | _T]]: ...
    # three iterables
    @overload
    def __new__(
        cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], /
    ) -> zip_longest[tuple[_T1 | MaybeNone, _T2 | MaybeNone, _T3 | MaybeNone]]: ...
    @overload
    def __new__(
        cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], /, *, fillvalue: _T
    ) -> zip_longest[tuple[_T1 | _T, _T2 | _T, _T3 | _T]]: ...
    # four iterables
    @overload
    def __new__(
        cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], iter4: Iterable[_T4], /
    ) -> zip_longest[tuple[_T1 | MaybeNone, _T2 | MaybeNone, _T3 | MaybeNone, _T4 | MaybeNone]]: ...
    @overload
    def __new__(
        cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], iter4: Iterable[_T4], /, *, fillvalue: _T
    ) -> zip_longest[tuple[_T1 | _T, _T2 | _T, _T3 | _T, _T4 | _T]]: ...
    # five iterables
    @overload
    def __new__(
        cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], iter4: Iterable[_T4], iter5: Iterable[_T5], /
    ) -> zip_longest[tuple[_T1 | MaybeNone, _T2 | MaybeNone, _T3 | MaybeNone, _T4 | MaybeNone, _T5 | MaybeNone]]: ...
    @overload
    def __new__(
        cls,
        iter1: Iterable[_T1],
        iter2: Iterable[_T2],
        iter3: Iterable[_T3],
        iter4: Iterable[_T4],
        iter5: Iterable[_T5],
        /,
        *,
        fillvalue: _T,
    ) -> zip_longest[tuple[_T1 | _T, _T2 | _T, _T3 | _T, _T4 | _T, _T5 | _T]]: ...
    # six or more iterables
    @overload
    def __new__(
        cls,
        iter1: Iterable[_T],
        iter2: Iterable[_T],
        iter3: Iterable[_T],
        iter4: Iterable[_T],
        iter5: Iterable[_T],
        iter6: Iterable[_T],
        /,
        *iterables: Iterable[_T],
    ) -> zip_longest[tuple[_T | MaybeNone, ...]]: ...
    @overload
    def __new__(
        cls,
        iter1: Iterable[_T],
        iter2: Iterable[_T],
        iter3: Iterable[_T],
        iter4: Iterable[_T],
        iter5: Iterable[_T],
        iter6: Iterable[_T],
        /,
        *iterables: Iterable[_T],
        fillvalue: _T,
    ) -> zip_longest[tuple[_T, ...]]: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T_co:
        """Implement next(self)."""

@disjoint_base
class product(Generic[_T_co]):
    """Cartesian product of input iterables.  Equivalent to nested for-loops.

    For example, product(A, B) returns the same as:  ((x,y) for x in A for y in B).
    The leftmost iterators are in the outermost for-loop, so the output tuples
    cycle in a manner similar to an odometer (with the rightmost element changing
    on every iteration).

    To compute the product of an iterable with itself, specify the number
    of repetitions with the optional repeat keyword argument. For example,
    product(A, repeat=4) means the same as product(A, A, A, A).

    product('ab', range(3)) --> ('a',0) ('a',1) ('a',2) ('b',0) ('b',1) ('b',2)
    product((0,1), (0,1), (0,1)) --> (0,0,0) (0,0,1) (0,1,0) (0,1,1) (1,0,0) ...
    """

    @overload
    def __new__(cls, iter1: Iterable[_T1], /) -> product[tuple[_T1]]: ...
    @overload
    def __new__(cls, iter1: Iterable[_T1], iter2: Iterable[_T2], /) -> product[tuple[_T1, _T2]]: ...
    @overload
    def __new__(cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], /) -> product[tuple[_T1, _T2, _T3]]: ...
    @overload
    def __new__(
        cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], iter4: Iterable[_T4], /
    ) -> product[tuple[_T1, _T2, _T3, _T4]]: ...
    @overload
    def __new__(
        cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], iter4: Iterable[_T4], iter5: Iterable[_T5], /
    ) -> product[tuple[_T1, _T2, _T3, _T4, _T5]]: ...
    @overload
    def __new__(
        cls,
        iter1: Iterable[_T1],
        iter2: Iterable[_T2],
        iter3: Iterable[_T3],
        iter4: Iterable[_T4],
        iter5: Iterable[_T5],
        iter6: Iterable[_T6],
        /,
    ) -> product[tuple[_T1, _T2, _T3, _T4, _T5, _T6]]: ...
    @overload
    def __new__(
        cls,
        iter1: Iterable[_T1],
        iter2: Iterable[_T2],
        iter3: Iterable[_T3],
        iter4: Iterable[_T4],
        iter5: Iterable[_T5],
        iter6: Iterable[_T6],
        iter7: Iterable[_T7],
        /,
    ) -> product[tuple[_T1, _T2, _T3, _T4, _T5, _T6, _T7]]: ...
    @overload
    def __new__(
        cls,
        iter1: Iterable[_T1],
        iter2: Iterable[_T2],
        iter3: Iterable[_T3],
        iter4: Iterable[_T4],
        iter5: Iterable[_T5],
        iter6: Iterable[_T6],
        iter7: Iterable[_T7],
        iter8: Iterable[_T8],
        /,
    ) -> product[tuple[_T1, _T2, _T3, _T4, _T5, _T6, _T7, _T8]]: ...
    @overload
    def __new__(
        cls,
        iter1: Iterable[_T1],
        iter2: Iterable[_T2],
        iter3: Iterable[_T3],
        iter4: Iterable[_T4],
        iter5: Iterable[_T5],
        iter6: Iterable[_T6],
        iter7: Iterable[_T7],
        iter8: Iterable[_T8],
        iter9: Iterable[_T9],
        /,
    ) -> product[tuple[_T1, _T2, _T3, _T4, _T5, _T6, _T7, _T8, _T9]]: ...
    @overload
    def __new__(
        cls,
        iter1: Iterable[_T1],
        iter2: Iterable[_T2],
        iter3: Iterable[_T3],
        iter4: Iterable[_T4],
        iter5: Iterable[_T5],
        iter6: Iterable[_T6],
        iter7: Iterable[_T7],
        iter8: Iterable[_T8],
        iter9: Iterable[_T9],
        iter10: Iterable[_T10],
        /,
    ) -> product[tuple[_T1, _T2, _T3, _T4, _T5, _T6, _T7, _T8, _T9, _T10]]: ...
    @overload
    def __new__(cls, *iterables: Iterable[_T1], repeat: int = 1) -> product[tuple[_T1, ...]]: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T_co:
        """Implement next(self)."""

@disjoint_base
class permutations(Generic[_T_co]):
    """Return successive r-length permutations of elements in the iterable.

    permutations(range(3), 2) --> (0,1), (0,2), (1,0), (1,2), (2,0), (2,1)
    """

    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[2]) -> permutations[tuple[_T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[3]) -> permutations[tuple[_T, _T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[4]) -> permutations[tuple[_T, _T, _T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[5]) -> permutations[tuple[_T, _T, _T, _T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: int | None = ...) -> permutations[tuple[_T, ...]]: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T_co:
        """Implement next(self)."""

@disjoint_base
class combinations(Generic[_T_co]):
    """Return successive r-length combinations of elements in the iterable.

    combinations(range(4), 3) --> (0,1,2), (0,1,3), (0,2,3), (1,2,3)
    """

    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[2]) -> combinations[tuple[_T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[3]) -> combinations[tuple[_T, _T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[4]) -> combinations[tuple[_T, _T, _T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[5]) -> combinations[tuple[_T, _T, _T, _T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: int) -> combinations[tuple[_T, ...]]: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T_co:
        """Implement next(self)."""

@disjoint_base
class combinations_with_replacement(Generic[_T_co]):
    """Return successive r-length combinations of elements in the iterable allowing individual elements to have successive repeats.

    combinations_with_replacement('ABC', 2) --> ('A','A'), ('A','B'), ('A','C'), ('B','B'), ('B','C'), ('C','C')
    """

    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[2]) -> combinations_with_replacement[tuple[_T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[3]) -> combinations_with_replacement[tuple[_T, _T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[4]) -> combinations_with_replacement[tuple[_T, _T, _T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: Literal[5]) -> combinations_with_replacement[tuple[_T, _T, _T, _T, _T]]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], r: int) -> combinations_with_replacement[tuple[_T, ...]]: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T_co:
        """Implement next(self)."""

if sys.version_info >= (3, 10):
    @disjoint_base
    class pairwise(Generic[_T_co]):
        """Return an iterator of overlapping pairs taken from the input iterator.

        s -> (s0,s1), (s1,s2), (s2, s3), ...
        """

        def __new__(cls, iterable: Iterable[_T], /) -> pairwise[tuple[_T, _T]]: ...
        def __iter__(self) -> Self:
            """Implement iter(self)."""

        def __next__(self) -> _T_co:
            """Implement next(self)."""

if sys.version_info >= (3, 12):
    @disjoint_base
    class batched(Generic[_T_co]):
        """Batch data into tuples of length n. The last batch may be shorter than n.

        Loops over the input iterable and accumulates data into tuples
        up to size n.  The input is consumed lazily, just enough to
        fill a batch.  The result is yielded as soon as a batch is full
        or when the input iterable is exhausted.

            >>> for batch in batched('ABCDEFG', 3):
            ...     print(batch)
            ...
            ('A', 'B', 'C')
            ('D', 'E', 'F')
            ('G',)

        If "strict" is True, raises a ValueError if the final batch is shorter
        than n.
        """

        if sys.version_info >= (3, 13):
            def __new__(cls, iterable: Iterable[_T_co], n: int, *, strict: bool = False) -> Self: ...
        else:
            def __new__(cls, iterable: Iterable[_T_co], n: int) -> Self: ...

        def __iter__(self) -> Self:
            """Implement iter(self)."""

        def __next__(self) -> tuple[_T_co, ...]:
            """Implement next(self)."""

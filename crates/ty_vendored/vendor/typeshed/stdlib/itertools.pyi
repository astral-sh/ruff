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
    @overload
    def __new__(cls) -> count[int]: ...
    @overload
    def __new__(cls, start: _N, step: _Step = ...) -> count[_N]: ...
    @overload
    def __new__(cls, *, step: _N) -> count[_N]: ...
    def __next__(self) -> _N: ...
    def __iter__(self) -> Self: ...

@disjoint_base
class cycle(Generic[_T]):
    def __new__(cls, iterable: Iterable[_T], /) -> Self: ...
    def __next__(self) -> _T: ...
    def __iter__(self) -> Self: ...

@disjoint_base
class repeat(Generic[_T]):
    @overload
    def __new__(cls, object: _T) -> Self: ...
    @overload
    def __new__(cls, object: _T, times: int) -> Self: ...
    def __next__(self) -> _T: ...
    def __iter__(self) -> Self: ...
    def __length_hint__(self) -> int: ...

@disjoint_base
class accumulate(Generic[_T]):
    @overload
    def __new__(cls, iterable: Iterable[_T], func: None = None, *, initial: _T | None = ...) -> Self: ...
    @overload
    def __new__(cls, iterable: Iterable[_S], func: Callable[[_T, _S], _T], *, initial: _T | None = ...) -> Self: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T: ...

@disjoint_base
class chain(Generic[_T]):
    def __new__(cls, *iterables: Iterable[_T]) -> Self: ...
    def __next__(self) -> _T: ...
    def __iter__(self) -> Self: ...
    @classmethod
    # We use type[Any] and not type[_S] to not lose the type inference from __iterable
    def from_iterable(cls: type[Any], iterable: Iterable[Iterable[_S]], /) -> chain[_S]: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

@disjoint_base
class compress(Generic[_T]):
    def __new__(cls, data: Iterable[_T], selectors: Iterable[Any]) -> Self: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T: ...

@disjoint_base
class dropwhile(Generic[_T]):
    def __new__(cls, predicate: _Predicate[_T], iterable: Iterable[_T], /) -> Self: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T: ...

@disjoint_base
class filterfalse(Generic[_T]):
    def __new__(cls, function: _Predicate[_T] | None, iterable: Iterable[_T], /) -> Self: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T: ...

@disjoint_base
class groupby(Generic[_T_co, _S_co]):
    @overload
    def __new__(cls, iterable: Iterable[_T1], key: None = None) -> groupby[_T1, _T1]: ...
    @overload
    def __new__(cls, iterable: Iterable[_T1], key: Callable[[_T1], _T2]) -> groupby[_T2, _T1]: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> tuple[_T_co, Iterator[_S_co]]: ...

@disjoint_base
class islice(Generic[_T]):
    @overload
    def __new__(cls, iterable: Iterable[_T], stop: int | None, /) -> Self: ...
    @overload
    def __new__(cls, iterable: Iterable[_T], start: int | None, stop: int | None, step: int | None = ..., /) -> Self: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T: ...

@disjoint_base
class starmap(Generic[_T_co]):
    def __new__(cls, function: Callable[..., _T], iterable: Iterable[Iterable[Any]], /) -> starmap[_T]: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T_co: ...

@disjoint_base
class takewhile(Generic[_T]):
    def __new__(cls, predicate: _Predicate[_T], iterable: Iterable[_T], /) -> Self: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T: ...

def tee(iterable: Iterable[_T], n: int = 2, /) -> tuple[Iterator[_T], ...]: ...
@disjoint_base
class zip_longest(Generic[_T_co]):
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
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T_co: ...

@disjoint_base
class product(Generic[_T_co]):
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
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T_co: ...

@disjoint_base
class permutations(Generic[_T_co]):
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
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T_co: ...

@disjoint_base
class combinations(Generic[_T_co]):
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
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T_co: ...

@disjoint_base
class combinations_with_replacement(Generic[_T_co]):
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
    def __iter__(self) -> Self: ...
    def __next__(self) -> _T_co: ...

if sys.version_info >= (3, 10):
    @disjoint_base
    class pairwise(Generic[_T_co]):
        def __new__(cls, iterable: Iterable[_T], /) -> pairwise[tuple[_T, _T]]: ...
        def __iter__(self) -> Self: ...
        def __next__(self) -> _T_co: ...

if sys.version_info >= (3, 12):
    @disjoint_base
    class batched(Generic[_T_co]):
        if sys.version_info >= (3, 13):
            def __new__(cls, iterable: Iterable[_T_co], n: int, *, strict: bool = False) -> Self: ...
        else:
            def __new__(cls, iterable: Iterable[_T_co], n: int) -> Self: ...

        def __iter__(self) -> Self: ...
        def __next__(self) -> tuple[_T_co, ...]: ...

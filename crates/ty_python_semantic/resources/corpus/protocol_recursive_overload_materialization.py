# /// script
# requires-python = ">=3.12"
# ///
# Comparing recursive protocol methods with many overloads should not grow combinatorially.

from __future__ import annotations

from collections.abc import Callable, Iterable, Iterator
from typing import Any, Literal, Never, Protocol, Self, overload


class Counter[T]:
    def __iadd__(self, other: Counter[T]) -> Self:
        raise NotImplementedError

    def __add__[Other](self, other: Counter[Other]) -> Counter[T | Other]:
        raise NotImplementedError


class Stream[T](Protocol):
    def __next__(self) -> T: ...
    def __iter__(self) -> Iterator[T]: ...

    @overload
    def map_star[A, R](
        self: Iterator[tuple[A]], function: Callable[[A], R]
    ) -> Stream[R]: ...
    @overload
    def map_star[A, B, R](
        self: Iterator[tuple[A, B]], function: Callable[[A, B], R]
    ) -> Stream[R]: ...
    @overload
    def map_star(self, function: Callable[..., Any]) -> Never: ...

    @overload
    def tag(self, label: Literal[0]) -> Stream[tuple[T, Literal[0]]]: ...
    @overload
    def tag(self, label: Literal[1]) -> Stream[tuple[T, Literal[1]]]: ...
    @overload
    def tag(self, label: Literal[2]) -> Stream[tuple[T, Literal[2]]]: ...
    @overload
    def tag(self, label: Literal[3]) -> Stream[tuple[T, Literal[3]]]: ...
    @overload
    def tag(self, label: Literal[4]) -> Stream[tuple[T, Literal[4]]]: ...
    @overload
    def tag(self, label: Literal[5]) -> Stream[tuple[T, Literal[5]]]: ...
    @overload
    def tag(self, label: Literal[6]) -> Stream[tuple[T, Literal[6]]]: ...
    @overload
    def tag(self, label: Literal[7]) -> Stream[tuple[T, Literal[7]]]: ...
    @overload
    def tag(self, label: Literal[8]) -> Stream[tuple[T, Literal[8]]]: ...


class Sequence[T]:
    def __init__(self, values: Iterable[T]) -> None: ...

    def iter(self) -> Stream[T]:
        raise NotImplementedError


operations = Sequence(((Counter[str].__iadd__, Counter[str].__add__),))
operations.iter().map_star(lambda first, second: 1)

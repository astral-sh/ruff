import collections.abc
import typing
from collections.abc import Iterator, Iterable


class NoReturn:
    def __iter__(self):
        ...


class TypingIterableTReturn:
    def __iter__(self) -> typing.Iterable[int]:
        ...

    def not_iter(self) -> typing.Iterable[int]:
        ...


class TypingIterableReturn:
    def __iter__(self) -> typing.Iterable:
        ...

    def not_iter(self) -> typing.Iterable:
        ...


class CollectionsIterableTReturn:
    def __iter__(self) -> collections.abc.Iterable[int]:
        ...

    def not_iter(self) -> collections.abc.Iterable[int]:
        ...


class CollectionsIterableReturn:
    def __iter__(self) -> collections.abc.Iterable:
        ...

    def not_iter(self) -> collections.abc.Iterable:
        ...


class IterableReturn:
    def __iter__(self) -> Iterable:
        ...


class IteratorReturn:
    def __iter__(self) -> Iterator:
        ...


class IteratorTReturn:
    def __iter__(self) -> Iterator[int]:
        ...


class TypingIteratorReturn:
    def __iter__(self) -> typing.Iterator:
        ...


class TypingIteratorTReturn:
    def __iter__(self) -> typing.Iterator[int]:
        ...


class CollectionsIteratorReturn:
    def __iter__(self) -> collections.abc.Iterator:
        ...


class CollectionsIteratorTReturn:
    def __iter__(self) -> collections.abc.Iterator[int]:
        ...


class TypingAsyncIterableTReturn:
    def __aiter__(self) -> typing.AsyncIterable[int]:
        ...


class TypingAsyncIterableReturn:
    def __aiter__(self) -> typing.AsyncIterable:
        ...

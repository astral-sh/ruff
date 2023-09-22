import collections.abc
import typing
from collections.abc import Iterator, Iterable

class NoReturn:
    def __iter__(self): ...

class TypingIterableTReturn:
    def __iter__(self) -> typing.Iterable[int]: ...  # Error: PYI045
    def not_iter(self) -> typing.Iterable[int]: ...

class TypingIterableReturn:
    def __iter__(self) -> typing.Iterable: ...  # Error: PYI045
    def not_iter(self) -> typing.Iterable: ...

class CollectionsIterableTReturn:
    def __iter__(self) -> collections.abc.Iterable[int]: ...  # Error: PYI045
    def not_iter(self) -> collections.abc.Iterable[int]: ...

class CollectionsIterableReturn:
    def __iter__(self) -> collections.abc.Iterable: ...  # Error: PYI045
    def not_iter(self) -> collections.abc.Iterable: ...

class IterableReturn:
    def __iter__(self) -> Iterable: ...  # Error: PYI045

class IteratorReturn:
    def __iter__(self) -> Iterator: ...

class IteratorTReturn:
    def __iter__(self) -> Iterator[int]: ...

class TypingIteratorReturn:
    def __iter__(self) -> typing.Iterator: ...

class TypingIteratorTReturn:
    def __iter__(self) -> typing.Iterator[int]: ...

class CollectionsIteratorReturn:
    def __iter__(self) -> collections.abc.Iterator: ...

class CollectionsIteratorTReturn:
    def __iter__(self) -> collections.abc.Iterator[int]: ...

class TypingAsyncIterableTReturn:
    def __aiter__(self) -> typing.AsyncIterable[int]: ...  # Error: PYI045

class TypingAsyncIterableReturn:
    def __aiter__(self) -> typing.AsyncIterable: ...  # Error: PYI045

# flags: --extend-ignore=Y023

import abc
import builtins
import collections.abc
import enum
import typing
from abc import ABCMeta, abstractmethod
from collections.abc import AsyncIterable, AsyncIterator, Iterable, Iterator
from enum import EnumMeta
from typing import Any, overload

import typing_extensions
from _typeshed import Self
from typing_extensions import final


class Bad(
    object
):  # Y040 Do not inherit from "object" explicitly, as it is redundant in Python 3
    def __new__(cls, *args: Any, **kwargs: Any) -> Bad:
        ...  # Y034 "__new__" methods usually return "self" at runtime. Consider using "typing_extensions.Self" in "Bad.__new__", e.g. "def __new__(cls, *args: Any, **kwargs: Any) -> Self: ..."

    def __repr__(self) -> str:
        ...  # Y029 Defining __repr__ or __str__ in a stub is almost always redundant

    def __str__(self) -> builtins.str:
        ...  # Y029 Defining __repr__ or __str__ in a stub is almost always redundant

    def __eq__(self, other: Any) -> bool:
        ...  # Y032 Prefer "object" to "Any" for the second parameter in "__eq__" methods

    def __ne__(self, other: typing.Any) -> typing.Any:
        ...  # Y032 Prefer "object" to "Any" for the second parameter in "__ne__" methods

    def __enter__(self) -> Bad:
        ...  # Y034 "__enter__" methods in classes like "Bad" usually return "self" at runtime. Consider using "typing_extensions.Self" in "Bad.__enter__", e.g. "def __enter__(self) -> Self: ..."

    async def __aenter__(self) -> Bad:
        ...  # Y034 "__aenter__" methods in classes like "Bad" usually return "self" at runtime. Consider using "typing_extensions.Self" in "Bad.__aenter__", e.g. "async def __aenter__(self) -> Self: ..."

    def __iadd__(self, other: Bad) -> Bad:
        ...  # Y034 "__iadd__" methods in classes like "Bad" usually return "self" at runtime. Consider using "typing_extensions.Self" in "Bad.__iadd__", e.g. "def __iadd__(self, other: Bad) -> Self: ..."


class AlsoBad(int, builtins.object):
    ...  # Y040 Do not inherit from "object" explicitly, as it is redundant in Python 3


class Good:
    def __new__(cls: type[Self], *args: Any, **kwargs: Any) -> Self:
        ...

    @abstractmethod
    def __str__(self) -> str:
        ...

    @abc.abstractmethod
    def __repr__(self) -> str:
        ...

    def __eq__(self, other: object) -> bool:
        ...

    def __ne__(self, obj: object) -> int:
        ...

    def __enter__(self: Self) -> Self:
        ...

    async def __aenter__(self: Self) -> Self:
        ...

    def __ior__(self: Self, other: Self) -> Self:
        ...


class Fine:
    @overload
    def __new__(cls, foo: int) -> FineSubclass:
        ...

    @overload
    def __new__(cls, *args: Any, **kwargs: Any) -> Fine:
        ...

    @abc.abstractmethod
    def __str__(self) -> str:
        ...

    @abc.abstractmethod
    def __repr__(self) -> str:
        ...

    def __eq__(self, other: Any, strange_extra_arg: list[str]) -> Any:
        ...

    def __ne__(self, *, kw_only_other: Any) -> bool:
        ...

    def __enter__(self) -> None:
        ...

    async def __aenter__(self) -> bool:
        ...


class FineSubclass(Fine):
    ...


class StrangeButAcceptable(str):
    @typing_extensions.overload
    def __new__(cls, foo: int) -> StrangeButAcceptableSubclass:
        ...

    @typing_extensions.overload
    def __new__(cls, *args: Any, **kwargs: Any) -> StrangeButAcceptable:
        ...

    def __str__(self) -> StrangeButAcceptable:
        ...

    def __repr__(self) -> StrangeButAcceptable:
        ...


class StrangeButAcceptableSubclass(StrangeButAcceptable):
    ...


class FineAndDandy:
    def __str__(self, weird_extra_arg) -> str:
        ...

    def __repr__(self, weird_extra_arg_with_default=...) -> str:
        ...


@final
class WillNotBeSubclassed:
    def __new__(cls, *args: Any, **kwargs: Any) -> WillNotBeSubclassed:
        ...

    def __enter__(self) -> WillNotBeSubclassed:
        ...

    async def __aenter__(self) -> WillNotBeSubclassed:
        ...


# we don't emit an error for these; out of scope for a linter
class InvalidButPluginDoesNotCrash:
    def __new__() -> InvalidButPluginDoesNotCrash:
        ...

    def __enter__() -> InvalidButPluginDoesNotCrash:
        ...

    async def __aenter__() -> InvalidButPluginDoesNotCrash:
        ...


class BadIterator1(Iterator[int]):
    def __iter__(self) -> Iterator[int]:
        ...  # Y034 "__iter__" methods in classes like "BadIterator1" usually return "self" at runtime. Consider using "typing_extensions.Self" in "BadIterator1.__iter__", e.g. "def __iter__(self) -> Self: ..."


class BadIterator2(
    typing.Iterator[int]
):  # Y022 Use "collections.abc.Iterator[T]" instead of "typing.Iterator[T]" (PEP 585 syntax)
    def __iter__(self) -> Iterator[int]:
        ...  # Y034 "__iter__" methods in classes like "BadIterator2" usually return "self" at runtime. Consider using "typing_extensions.Self" in "BadIterator2.__iter__", e.g. "def __iter__(self) -> Self: ..."


class BadIterator3(
    typing.Iterator[int]
):  # Y022 Use "collections.abc.Iterator[T]" instead of "typing.Iterator[T]" (PEP 585 syntax)
    def __iter__(self) -> collections.abc.Iterator[int]:
        ...  # Y034 "__iter__" methods in classes like "BadIterator3" usually return "self" at runtime. Consider using "typing_extensions.Self" in "BadIterator3.__iter__", e.g. "def __iter__(self) -> Self: ..."


class BadIterator4(Iterator[int]):
    # Note: *Iterable*, not *Iterator*, returned!
    def __iter__(self) -> Iterable[int]:
        ...  # Y034 "__iter__" methods in classes like "BadIterator4" usually return "self" at runtime. Consider using "typing_extensions.Self" in "BadIterator4.__iter__", e.g. "def __iter__(self) -> Self: ..."


class IteratorReturningIterable:
    def __iter__(self) -> Iterable[str]:
        ...  # Y045 "__iter__" methods should return an Iterator, not an Iterable


class BadAsyncIterator(collections.abc.AsyncIterator[str]):
    def __aiter__(self) -> typing.AsyncIterator[str]:
        ...  # Y034 "__aiter__" methods in classes like "BadAsyncIterator" usually return "self" at runtime. Consider using "typing_extensions.Self" in "BadAsyncIterator.__aiter__", e.g. "def __aiter__(self) -> Self: ..."  # Y022 Use "collections.abc.AsyncIterator[T]" instead of "typing.AsyncIterator[T]" (PEP 585 syntax)

class SubclassOfBadIterator3(BadIterator3):
    def __iter__(self) -> Iterator[int]:  # Y034
        ...

class SubclassOfBadAsyncIterator(BadAsyncIterator):
    def __aiter__(self) -> collections.abc.AsyncIterator[str]:  # Y034
        ...

class AsyncIteratorReturningAsyncIterable:
    def __aiter__(self) -> AsyncIterable[str]:
        ...  # Y045 "__aiter__" methods should return an AsyncIterator, not an AsyncIterable


class MetaclassInWhichSelfCannotBeUsed(type):
    def __new__(cls) -> MetaclassInWhichSelfCannotBeUsed: ...
    def __enter__(self) -> MetaclassInWhichSelfCannotBeUsed: ...
    async def __aenter__(self) -> MetaclassInWhichSelfCannotBeUsed: ...
    def __isub__(self, other: MetaclassInWhichSelfCannotBeUsed) -> MetaclassInWhichSelfCannotBeUsed: ...

class MetaclassInWhichSelfCannotBeUsed2(EnumMeta):
    def __new__(cls) -> MetaclassInWhichSelfCannotBeUsed2: ...
    def __enter__(self) -> MetaclassInWhichSelfCannotBeUsed2: ...
    async def __aenter__(self) -> MetaclassInWhichSelfCannotBeUsed2: ...
    def __isub__(self, other: MetaclassInWhichSelfCannotBeUsed2) -> MetaclassInWhichSelfCannotBeUsed2: ...

class MetaclassInWhichSelfCannotBeUsed3(enum.EnumType):
    def __new__(cls) -> MetaclassInWhichSelfCannotBeUsed3: ...
    def __enter__(self) -> MetaclassInWhichSelfCannotBeUsed3: ...
    async def __aenter__(self) -> MetaclassInWhichSelfCannotBeUsed3: ...
    def __isub__(self, other: MetaclassInWhichSelfCannotBeUsed3) -> MetaclassInWhichSelfCannotBeUsed3: ...

class MetaclassInWhichSelfCannotBeUsed4(ABCMeta):
    def __new__(cls) -> MetaclassInWhichSelfCannotBeUsed4: ...
    def __enter__(self) -> MetaclassInWhichSelfCannotBeUsed4: ...
    async def __aenter__(self) -> MetaclassInWhichSelfCannotBeUsed4: ...
    def __isub__(self, other: MetaclassInWhichSelfCannotBeUsed4) -> MetaclassInWhichSelfCannotBeUsed4: ...

class SubclassOfMetaclassInWhichSelfCannotBeUsed(MetaclassInWhichSelfCannotBeUsed4):
    def __new__(cls) -> SubclassOfMetaclassInWhichSelfCannotBeUsed: ...
    def __enter__(self) -> SubclassOfMetaclassInWhichSelfCannotBeUsed: ...
    async def __aenter__(self) -> SubclassOfMetaclassInWhichSelfCannotBeUsed: ...
    def __isub__(self, other: SubclassOfMetaclassInWhichSelfCannotBeUsed) -> SubclassOfMetaclassInWhichSelfCannotBeUsed: ...

class Abstract(Iterator[str]):
    @abstractmethod
    def __iter__(self) -> Iterator[str]:
        ...

    @abstractmethod
    def __enter__(self) -> Abstract:
        ...

    @abstractmethod
    async def __aenter__(self) -> Abstract:
        ...


class GoodIterator(Iterator[str]):
    def __iter__(self: Self) -> Self:
        ...


class GoodAsyncIterator(AsyncIterator[int]):
    def __aiter__(self: Self) -> Self:
        ...


class DoesNotInheritFromIterator:
    def __iter__(self) -> DoesNotInheritFromIterator:
        ...


class Unannotated:
    def __new__(cls, *args, **kwargs):
        ...

    def __iter__(self):
        ...

    def __aiter__(self):
        ...

    async def __aenter__(self):
        ...

    def __repr__(self):
        ...

    def __str__(self):
        ...

    def __eq__(self):
        ...

    def __ne__(self):
        ...

    def __iadd__(self):
        ...

    def __ior__(self):
        ...


def __repr__(self) -> str:
    ...


def __str__(self) -> str:
    ...


def __eq__(self, other: Any) -> bool:
    ...


def __ne__(self, other: Any) -> bool:
    ...


def __imul__(self, other: Any) -> list[str]:
    ...

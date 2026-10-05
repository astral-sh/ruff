# /// script
# requires-python = ">=3.12"
# ///
# Receiver overload filtering and materialization must terminate for recursive protocols.

from __future__ import annotations
from typing import Any, Generator, Protocol, overload


class Deferred[T](Protocol):
    def __await__(self) -> Generator[object, Any, T]: ...


class Stream[T](Deferred[T], Protocol):
    @overload
    def flatten[U](self: Stream[Deferred[U]]) -> Stream[U]: ...

    @overload
    def flatten(self) -> object: ...

    def nested(self) -> Stream[Stream[T]]: ...


class DerivedStream[ItemT](Stream[ItemT], Protocol): ...


class Provider(Protocol):
    def make[ItemT](self) -> DerivedStream[ItemT]: ...


class Adapter[ValueT: Provider]:
    _value: ValueT

    @property
    def ready(self): ...

# /// script
# requires-python = ">=3.12"
# ///
# Binding methods on nested recursive protocols must terminate.

from __future__ import annotations

from typing import Any, AsyncIterable, Callable, Iterable, Protocol, overload


class Source[T](AsyncIterable[T], Protocol):
    async def to_map[K](self, key: Callable[[T], K]) -> dict[K, T]: ...

    @overload
    def flatten[U](
        self: Source[Source[U] | AsyncIterable[U] | Iterable[U]], levels: int = 1
    ) -> Source[U]: ...

    @overload
    def flatten(self, levels: int = 1) -> Source[Any]: ...

    def flatten(self, levels: int = 1) -> Source[Any]: ...

    def index(self) -> Source[tuple[int, T]]: ...


class Flow[T](Protocol):
    def pipe[U](self, transform: Callable[[Source[T]], Source[U]]) -> Flow[U]: ...


class Record(Protocol): ...


class RecordCollection(Source[Record], Protocol): ...


class Item(Protocol):
    def records(self) -> RecordCollection: ...


class View(Protocol):
    @property
    def value(self) -> int: ...

    def items[Element: Item](self) -> Flow[Element]: ...

    async def get_item[Element: Item](self) -> Element | Item | None: ...


class Wrapper[Wrapped]:
    def __init__(self, delegate: Wrapped) -> None:
        self._delegate = delegate

    @property
    def delegate(self) -> Wrapped:
        return self._delegate


def check[Wrapped: View](wrapper: Wrapper[Wrapped]) -> int:
    return wrapper.delegate.value

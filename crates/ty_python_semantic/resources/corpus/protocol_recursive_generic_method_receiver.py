# /// script
# requires-python = ">=3.12"
# ///
# A generic receiver in a recursive protocol must not cause type mapping to recurse indefinitely.

from __future__ import annotations

from typing import Any, Protocol, overload


class Stream[T](Protocol):
    def to_list(self) -> list[T]: ...

    @overload
    def flatten[U](self: Stream[U]) -> Stream[U]: ...

    @overload
    def flatten(self) -> Stream[Any]: ...

    def window(self) -> Stream[Stream[T]]: ...


class ElementView(Protocol):
    def payloads(self) -> Stream[object]: ...


class View(Protocol):
    timestamp: int

    def get_item(self) -> ElementView: ...


class Wrapper[DelegateT: View]:
    _delegate: DelegateT

    @property
    def timestamp(self) -> int:
        return self._delegate.timestamp

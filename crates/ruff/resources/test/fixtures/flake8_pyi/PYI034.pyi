from typing_extensions import Self
from abc import abstractmethod
from typing import Any, overload, final
from collections.abc import Iterator, Iterable, AsyncIterator


class Bad:
  def __new__(cls, *args: Any, **kwargs: Any) -> Bad: ...  # Y034
  def __enter__(self) -> Bad: ...  # Y034
  async def __aenter__(self) -> Bad: ...  # Y034
  def __iadd__(self, other: Bad) -> Bad: ...  # Y034

  class Good:
    def __new__(cls: type[Self], *args: Any, **kwargs: Any) -> Self: ...
    def __enter__(self: Self) -> Self: ...
    async def __aenter__(self: Self) -> Self: ...
    def __ior__(self: Self, other: Self) -> Self: ...


class BadButIgnoredBecauseOverloaded:
  @overload
  def __new__(cls, *args: Any, **kwargs: Any) -> Bad: ...  # OK
  @overload
  def __enter__(self) -> Bad: ...  # OK
  @abstractmethod
  async def __aenter__(self) -> Bad: ...  # Ok
  @abstractmethod
  def __iadd__(self, other: Bad) -> Bad: ...  # Ok


@final
class WillNotBeSubclassed:
    def __new__(cls, *args: Any, **kwargs: Any) -> WillNotBeSubclassed: ...
    def __enter__(self) -> WillNotBeSubclassed: ...
    async def __aenter__(self) -> WillNotBeSubclassed: ...



class BadIterator1(Iterator[int]):
  def __iter__(self) -> Iterator[int]: ...  # Y034


class BadIterator2(Iterator[int]):
    # Note: *Iterable*, not *Iterator*, returned!
    def __iter__(self) -> Iterable[int]: ...  # Y034


class BadAsyncIterator(AsyncIterator[str]):
    def __aiter__(self) -> AsyncIterator[str]: ...  # Y034


class Unannotated:
    def __new__(cls, *args, **kwargs): ...
    def __iter__(self): ...
    def __aiter__(self): ...
    async def __aenter__(self): ...


@final
class WillNotBeSubclassed:
    def __new__(cls, *args: Any, **kwargs: Any) -> WillNotBeSubclassed: ...
    def __enter__(self) -> WillNotBeSubclassed: ...
    async def __aenter__(self) -> WillNotBeSubclassed: ...

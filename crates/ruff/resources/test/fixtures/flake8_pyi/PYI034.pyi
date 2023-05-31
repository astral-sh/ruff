from typing_extensions import Self
from abc import abstractmethod
from typing import Any, overload, final
from collections.abc import Iterator, Iterable, AsyncIterator


class Bad:
  def __new__(cls, *args: Any, **kwargs: Any) -> Bad: ...  # Y034 "__new__" methods usually return "self" at runtime. Consider using "typing_extensions.Self" in "Bad.__new__", e.g. "def __new__(cls, *args: Any, **kwargs: Any) -> Self: ..."
  def __enter__(self) -> Bad: ...  # Y034 "__enter__" methods in classes like "Bad" usually return "self" at runtime. Consider using "typing_extensions.Self" in "Bad.__enter__", e.g. "def __enter__(self) -> Self: ..."
  async def __aenter__(self) -> Bad: ...  # Y034 "__aenter__" methods in classes like "Bad" usually return "self" at runtime. Consider using "typing_extensions.Self" in "Bad.__aenter__", e.g. "async def __aenter__(self) -> Self: ..."
  def __iadd__(self, other: Bad) -> Bad: ...  # Y034 "__iadd__" methods in classes like "Bad" usually return "self" at runtime. Consider using "typing_extensions.Self" in "Bad.__iadd__", e.g. "def __iadd__(self, other: Bad) -> Self: ..."

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
  def __iter__(self) -> Iterator[int]: ...  # Y034 "__iter__" methods in classes like "BadIterator1" usually return "self" at runtime. Consider using "typing_extensions.Self" in "BadIterator1.__iter__", e.g. "def __iter__(self) -> Self: ..."


class BadIterator2(Iterator[int]):
    # Note: *Iterable*, not *Iterator*, returned!
    def __iter__(self) -> Iterable[int]: ...  # Y034 "__iter__" methods in classes like "BadIterator4" usually return "self" at runtime. Consider using "typing_extensions.Self" in "BadIterator4.__iter__", e.g. "def __iter__(self) -> Self: ..."



class BadAsyncIterator(AsyncIterator[str]):
    def __aiter__(self) -> AsyncIterator[str]: ...  # Y034 "__aiter__" methods in classes like "BadAsyncIterator" usually return "self" at runtime. Consider using "typing_extensions.Self" in "BadAsyncIterator.__aiter__", e.g. "def __aiter__(self) -> Self: ..."

class Unannotated:
    def __new__(cls, *args, **kwargs): ...
    def __iter__(self): ...
    def __aiter__(self): ...
    async def __aenter__(self): ...

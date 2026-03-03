# Decorated Stream Protocol Regression

```toml
[environment]
python-version = "3.12"
```

`streamkit/__init__.py`:

```py
# package marker
```

`streamkit/types.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING, AsyncContextManager, ContextManager, TypeGuard
from collections.abc import AsyncIterable, Awaitable, Callable, Iterable

if TYPE_CHECKING:
    from .protocol import Streamable

type SyncMapper[T, U] = Callable[[T], U]
type AsyncMapper[T, U] = Callable[[T], Awaitable[U]]
type Mapper[T, U] = SyncMapper[T, U] | AsyncMapper[T, U]
type SyncPredicate[T, U] = Callable[[T], TypeGuard[U] | bool]
type AsyncPredicate[T, U] = Callable[[T], Awaitable[TypeGuard[U] | bool]]
type Predicate[T, U] = SyncPredicate[T, U] | AsyncPredicate[T, U]
type SyncBoolPredicate[T] = Callable[[T], bool]
type AsyncBoolPredicate[T] = Callable[[T], Awaitable[bool]]
type BoolPredicate[T] = SyncBoolPredicate[T] | AsyncBoolPredicate[T]
type SyncTypeGuardPredicate[T, U] = Callable[[T], TypeGuard[U]]
type AsyncTypeGuardPredicate[T, U] = Callable[[T], Awaitable[TypeGuard[U]]]
type TypeGuardPredicate[T, U] = SyncTypeGuardPredicate[T, U] | AsyncTypeGuardPredicate[T, U]
type SyncSelector[T, U] = Callable[[T], StreamLike[U]]
type AsyncSelector[T, U] = Callable[[T], Awaitable[StreamLike[U]]]
type Selector[T, U] = SyncSelector[T, U] | AsyncSelector[T, U]
type StreamTransform[T, U] = Callable[[Streamable[T]], Streamable[U]]
type StreamLike[T] = Streamable[T] | AsyncIterable[T] | Iterable[T]
type SyncAccumulator[A, T] = Callable[[A, T], A]
type AsyncAccumulator[A, T] = Callable[[A, T], Awaitable[A]]
type Accumulator[A, T] = SyncAccumulator[A, T] | AsyncAccumulator[A, T]
type SyncCollectAccumulator[T] = Callable[[T], None]
type AsyncCollectAccumulator[T] = Callable[[T], Awaitable[None]]
type CollectAccumulator[T] = SyncCollectAccumulator[T] | AsyncCollectAccumulator[T]
type SyncFinisher[R] = Callable[[], R]
type AsyncFinisher[R] = Callable[[], Awaitable[R]]
type Finisher[R] = SyncFinisher[R] | AsyncFinisher[R]
type Collector[T, R] = tuple[CollectAccumulator[T], Finisher[R]]
type SyncSideEffect[T] = Callable[[T], None]
type AsyncSideEffect[T] = Callable[[T], Awaitable[None]]
type SideEffect[T] = SyncSideEffect[T] | AsyncSideEffect[T]
type SyncKeySelector[T, K] = Callable[[T], K]
type AsyncKeySelector[T, K] = Callable[[T], Awaitable[K]]
type KeySelector[T, K] = SyncKeySelector[T, K] | AsyncKeySelector[T, K]
type ContextManagerFactory = Callable[[], ContextManager[object] | AsyncContextManager[object]]
```

`streamkit/_utils.py`:

```py
from __future__ import annotations

import inspect
from typing import Awaitable, cast


async def _await_if_needed[T](value: T | Awaitable[T]) -> T:
    if inspect.isawaitable(value):
        return await cast(Awaitable[T], value)
    return value
```

`streamkit/protocol.py`:

```py
from __future__ import annotations

from collections.abc import AsyncIterable
from typing import Any, Protocol, overload

from .types import (
    Accumulator,
    BoolPredicate,
    CollectAccumulator,
    Collector,
    Finisher,
    KeySelector,
    Mapper,
    Predicate,
    Selector,
    StreamLike,
    StreamTransform,
    TypeGuardPredicate,
)


class Streamable[T](AsyncIterable[T], Protocol):
    async def has_any(self) -> bool: ...

    @overload
    async def has_any_matching[U1](self, predicate: Predicate[T, U1], /) -> bool: ...

    @overload
    async def has_any_matching[U1, U2](
        self,
        predicate1: Predicate[T, U1],
        predicate2: Predicate[T, U2],
        /,
    ) -> tuple[bool, bool]: ...

    @overload
    async def has_any_matching[U1, U2, U3](
        self,
        predicate1: Predicate[T, U1],
        predicate2: Predicate[T, U2],
        predicate3: Predicate[T, U3],
        /,
    ) -> tuple[bool, bool, bool]: ...

    @overload
    async def has_any_matching[U1, U2, U3, U4](
        self,
        predicate1: Predicate[T, U1],
        predicate2: Predicate[T, U2],
        predicate3: Predicate[T, U3],
        predicate4: Predicate[T, U4],
        /,
    ) -> tuple[bool, bool, bool, bool]: ...

    @overload
    async def has_any_matching[U1, U2, U3, U4, U5](
        self,
        predicate1: Predicate[T, U1],
        predicate2: Predicate[T, U2],
        predicate3: Predicate[T, U3],
        predicate4: Predicate[T, U4],
        predicate5: Predicate[T, U5],
        /,
    ) -> tuple[bool, bool, bool, bool, bool]: ...

    @overload
    async def has_any_matching(self, *predicates: Predicate[T, Any]) -> tuple[bool, ...]: ...

    async def has_any_matching(self, *predicates: Predicate[T, Any]) -> Any: ...

    async def to_list(self) -> list[T]: ...

    @overload
    async def collect[R](self, accumulator: CollectAccumulator[T], finisher: Finisher[R], /) -> R: ...

    @overload
    async def collect[R1](self, collector: Collector[T, R1], /) -> R1: ...

    @overload
    async def collect[R1, R2](
        self,
        collector1: Collector[T, R1],
        collector2: Collector[T, R2],
        /,
    ) -> tuple[R1, R2]: ...

    @overload
    async def collect[R1, R2, R3](
        self,
        collector1: Collector[T, R1],
        collector2: Collector[T, R2],
        collector3: Collector[T, R3],
        /,
    ) -> tuple[R1, R2, R3]: ...

    async def collect(self, *collectors: object) -> Any: ...

    @overload
    async def first_matching(self, predicate: BoolPredicate[T], /) -> T | None: ...

    @overload
    async def first_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None]: ...

    @overload
    async def first_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None]: ...

    @overload
    async def first_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        predicate4: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None, T | None]: ...

    @overload
    async def first_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        predicate4: BoolPredicate[T],
        predicate5: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None, T | None, T | None]: ...

    @overload
    async def first_matching[U1](self, predicate: TypeGuardPredicate[T, U1], /) -> U1 | None: ...

    @overload
    async def first_matching[U1, U2](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        /,
    ) -> tuple[U1 | None, U2 | None]: ...

    @overload
    async def first_matching[U1, U2, U3](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None]: ...

    @overload
    async def first_matching[U1, U2, U3, U4](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        predicate4: TypeGuardPredicate[T, U4],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None, U4 | None]: ...

    @overload
    async def first_matching[U1, U2, U3, U4, U5](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        predicate4: TypeGuardPredicate[T, U4],
        predicate5: TypeGuardPredicate[T, U5],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None, U4 | None, U5 | None]: ...

    @overload
    async def first_matching(self, *predicates: Predicate[T, Any]) -> tuple[Any, ...]: ...

    async def first_matching(self, *predicates: Predicate[T, Any]) -> Any: ...

    @overload
    async def last_matching(self, predicate: BoolPredicate[T], /) -> T | None: ...

    @overload
    async def last_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None]: ...

    @overload
    async def last_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None]: ...

    @overload
    async def last_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        predicate4: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None, T | None]: ...

    @overload
    async def last_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        predicate4: BoolPredicate[T],
        predicate5: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None, T | None, T | None]: ...

    @overload
    async def last_matching[U1](self, predicate: TypeGuardPredicate[T, U1], /) -> U1 | None: ...

    @overload
    async def last_matching[U1, U2](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        /,
    ) -> tuple[U1 | None, U2 | None]: ...

    @overload
    async def last_matching[U1, U2, U3](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None]: ...

    @overload
    async def last_matching[U1, U2, U3, U4](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        predicate4: TypeGuardPredicate[T, U4],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None, U4 | None]: ...

    @overload
    async def last_matching[U1, U2, U3, U4, U5](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        predicate4: TypeGuardPredicate[T, U4],
        predicate5: TypeGuardPredicate[T, U5],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None, U4 | None, U5 | None]: ...

    @overload
    async def last_matching(self, *predicates: Predicate[T, Any]) -> tuple[Any, ...]: ...

    async def last_matching(self, *predicates: Predicate[T, Any]) -> Any: ...

    def filter[U](self, predicate: Predicate[T, U], /) -> "Streamable[U]": ...

    def map[U](self, func: Mapper[T, U], /) -> "Streamable[U]": ...

    def flat_map[U](self, selector: Selector[T, U], /) -> "Streamable[U]": ...

    def pipe[U](self, transform: StreamTransform[T, U], /) -> "Streamable[U]": ...

    def index(self) -> "Streamable[tuple[int, T]]": ...

    def scan[A](self, initial: A, accumulator: Accumulator[A, T]) -> "Streamable[A]": ...

    def sort[K](self, key: KeySelector[T, K] | None = None, reverse: bool = False) -> "Streamable[T]": ...

    def distinct[K](self, key: KeySelector[T, K] | None = None) -> "Streamable[T]": ...

    def skip(self, count: int) -> "Streamable[T]": ...

    def skip_last(self, count: int) -> "Streamable[T]": ...

    def skip_until[U](self, predicate: Predicate[T, U], /) -> "Streamable[T]": ...

    def skip_while[U](self, predicate: Predicate[T, U], /) -> "Streamable[T]": ...

    def take(self, count: int) -> "Streamable[T]": ...

    def take_last(self, count: int) -> "Streamable[T]": ...

    def take_until[U](self, predicate: Predicate[T, U], /) -> "Streamable[T]": ...

    def take_while[U](self, predicate: Predicate[T, U], /) -> "Streamable[T]": ...

    def concat(self, *others: StreamLike[T]) -> "Streamable[T]": ...

    def merge(self, *others: StreamLike[T]) -> "Streamable[T]": ...
```

`streamkit/streamables.py`:

```py
from __future__ import annotations

from collections import deque
from collections.abc import AsyncIterable, AsyncIterator, Awaitable, Callable, Iterable
from typing import Any, cast, overload

from ._utils import _await_if_needed
from .protocol import Streamable
from .types import (
    Accumulator,
    BoolPredicate,
    CollectAccumulator,
    Collector,
    ContextManagerFactory,
    Finisher,
    KeySelector,
    Mapper,
    Predicate,
    Selector,
    SideEffect,
    StreamLike,
    StreamTransform,
    TypeGuardPredicate,
)


def _ensure_async_iterable[T](iterable: Iterable[T] | AsyncIterable[T]) -> AsyncIterable[T]:
    if isinstance(iterable, AsyncIterable):
        return cast(AsyncIterable[T], iterable)
    return _IterableAsyncIterable(iterable)


class _IterableAsyncIterable[T](AsyncIterable[T]):
    def __init__(self, iterable: Iterable[T]) -> None:
        self._iterable = iterable

    async def __aiter__(self) -> AsyncIterator[T]:
        for item in self._iterable:
            yield item


class _StreamableImpl[T](Streamable[T]):
    def __init__(
        self,
        source: AsyncIterable[T] | Iterable[T] | Awaitable[AsyncIterable[T] | Iterable[T]],
    ) -> None:
        self._source = source

    def __aiter__(self) -> AsyncIterator[T]:
        raise NotImplementedError

    @staticmethod
    def _wrap[U](source: AsyncIterable[U]) -> Streamable[U]:
        return _StreamableImpl(source)

    async def has_any(self) -> bool:
        raise NotImplementedError

    @overload
    async def has_any_matching[U1](self, predicate: Predicate[T, U1], /) -> bool: ...

    @overload
    async def has_any_matching[U1, U2](
        self,
        predicate1: Predicate[T, U1],
        predicate2: Predicate[T, U2],
        /,
    ) -> tuple[bool, bool]: ...

    @overload
    async def has_any_matching[U1, U2, U3](
        self,
        predicate1: Predicate[T, U1],
        predicate2: Predicate[T, U2],
        predicate3: Predicate[T, U3],
        /,
    ) -> tuple[bool, bool, bool]: ...

    @overload
    async def has_any_matching[U1, U2, U3, U4](
        self,
        predicate1: Predicate[T, U1],
        predicate2: Predicate[T, U2],
        predicate3: Predicate[T, U3],
        predicate4: Predicate[T, U4],
        /,
    ) -> tuple[bool, bool, bool, bool]: ...

    @overload
    async def has_any_matching[U1, U2, U3, U4, U5](
        self,
        predicate1: Predicate[T, U1],
        predicate2: Predicate[T, U2],
        predicate3: Predicate[T, U3],
        predicate4: Predicate[T, U4],
        predicate5: Predicate[T, U5],
        /,
    ) -> tuple[bool, bool, bool, bool, bool]: ...

    @overload
    async def has_any_matching(self, *predicates: Predicate[T, Any]) -> tuple[bool, ...]: ...

    async def has_any_matching(self, *predicates: Predicate[T, Any]) -> Any:
        raise NotImplementedError

    async def count(self) -> int:
        raise NotImplementedError

    async def first(self) -> T | None:
        raise NotImplementedError

    @overload
    async def first_matching(self, predicate: BoolPredicate[T], /) -> T | None: ...

    @overload
    async def first_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None]: ...

    @overload
    async def first_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None]: ...

    @overload
    async def first_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        predicate4: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None, T | None]: ...

    @overload
    async def first_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        predicate4: BoolPredicate[T],
        predicate5: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None, T | None, T | None]: ...

    @overload
    async def first_matching[U1](self, predicate: TypeGuardPredicate[T, U1], /) -> U1 | None: ...

    @overload
    async def first_matching[U1, U2](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        /,
    ) -> tuple[U1 | None, U2 | None]: ...

    @overload
    async def first_matching[U1, U2, U3](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None]: ...

    @overload
    async def first_matching[U1, U2, U3, U4](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        predicate4: TypeGuardPredicate[T, U4],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None, U4 | None]: ...

    @overload
    async def first_matching[U1, U2, U3, U4, U5](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        predicate4: TypeGuardPredicate[T, U4],
        predicate5: TypeGuardPredicate[T, U5],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None, U4 | None, U5 | None]: ...

    @overload
    async def first_matching(self, *predicates: Predicate[T, Any]) -> tuple[Any, ...]: ...

    async def first_matching(self, *predicates: Predicate[T, Any]) -> Any:
        raise NotImplementedError

    @overload
    async def last_matching(self, predicate: BoolPredicate[T], /) -> T | None: ...

    @overload
    async def last_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None]: ...

    @overload
    async def last_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None]: ...

    @overload
    async def last_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        predicate4: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None, T | None]: ...

    @overload
    async def last_matching(
        self,
        predicate1: BoolPredicate[T],
        predicate2: BoolPredicate[T],
        predicate3: BoolPredicate[T],
        predicate4: BoolPredicate[T],
        predicate5: BoolPredicate[T],
        /,
    ) -> tuple[T | None, T | None, T | None, T | None, T | None]: ...

    @overload
    async def last_matching[U1](self, predicate: TypeGuardPredicate[T, U1], /) -> U1 | None: ...

    @overload
    async def last_matching[U1, U2](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        /,
    ) -> tuple[U1 | None, U2 | None]: ...

    @overload
    async def last_matching[U1, U2, U3](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None]: ...

    @overload
    async def last_matching[U1, U2, U3, U4](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        predicate4: TypeGuardPredicate[T, U4],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None, U4 | None]: ...

    @overload
    async def last_matching[U1, U2, U3, U4, U5](
        self,
        predicate1: TypeGuardPredicate[T, U1],
        predicate2: TypeGuardPredicate[T, U2],
        predicate3: TypeGuardPredicate[T, U3],
        predicate4: TypeGuardPredicate[T, U4],
        predicate5: TypeGuardPredicate[T, U5],
        /,
    ) -> tuple[U1 | None, U2 | None, U3 | None, U4 | None, U5 | None]: ...

    @overload
    async def last_matching(self, *predicates: Predicate[T, Any]) -> tuple[Any, ...]: ...

    async def last_matching(self, *predicates: Predicate[T, Any]) -> Any:
        raise NotImplementedError

    async def last(self) -> T | None:
        raise NotImplementedError

    async def reduce[A](self, initial: A, accumulator: Accumulator[A, T]) -> A:
        raise NotImplementedError

    async def to_list(self) -> list[T]:
        raise NotImplementedError

    @overload
    async def collect[R](
        self, accumulator: CollectAccumulator[T], finisher: Finisher[R], /
    ) -> R: ...

    @overload
    async def collect[R1](self, collector: Collector[T, R1], /) -> R1: ...

    @overload
    async def collect[R1, R2](
        self,
        collector1: Collector[T, R1],
        collector2: Collector[T, R2],
        /,
    ) -> tuple[R1, R2]: ...

    @overload
    async def collect[R1, R2, R3](
        self,
        collector1: Collector[T, R1],
        collector2: Collector[T, R2],
        collector3: Collector[T, R3],
        /,
    ) -> tuple[R1, R2, R3]: ...

    async def collect(self, *collectors: object) -> Any:
        raise NotImplementedError

    async def to_map[K](self, key: KeySelector[T, K]) -> dict[K, T]:
        raise NotImplementedError

    def filter[U](self, predicate: Predicate[T, U], /) -> Streamable[U]:
        raise NotImplementedError

    def map[U](self, func: Mapper[T, U], /) -> Streamable[U]:
        raise NotImplementedError

    def flat_map[U](self, selector: Selector[T, U], /) -> Streamable[U]:
        raise NotImplementedError

    def pipe[U](self, transform: StreamTransform[T, U], /) -> Streamable[U]:
        raise NotImplementedError

    def index(self) -> Streamable[tuple[int, T]]:
        raise NotImplementedError

    def on_each(self, effect: SideEffect[T]) -> Streamable[T]:
        raise NotImplementedError

    def all_in_context(self, factory: ContextManagerFactory) -> Streamable[T]:
        raise NotImplementedError

    def each_in_context(self, factory: ContextManagerFactory) -> Streamable[T]:
        raise NotImplementedError

    def scan[A](self, initial: A, accumulator: Accumulator[A, T]) -> Streamable[A]:
        raise NotImplementedError

    def sort[K](self, key: KeySelector[T, K] | None = None, reverse: bool = False) -> Streamable[T]:
        raise NotImplementedError

    def distinct[K](self, key: KeySelector[T, K] | None = None) -> Streamable[T]:
        raise NotImplementedError

    def skip(self, count: int) -> Streamable[T]:
        raise NotImplementedError

    def skip_last(self, count: int) -> Streamable[T]:
        raise NotImplementedError

    def skip_until[U](self, predicate: Predicate[T, U], /) -> Streamable[T]:
        raise NotImplementedError

    def skip_while[U](self, predicate: Predicate[T, U], /) -> Streamable[T]:
        raise NotImplementedError

    def take(self, count: int) -> Streamable[T]:
        raise NotImplementedError

    def take_last(self, count: int) -> Streamable[T]:
        raise NotImplementedError

    def take_until[U](self, predicate: Predicate[T, U], /) -> Streamable[T]:
        raise NotImplementedError

    def take_while[U](self, predicate: Predicate[T, U], /) -> Streamable[T]:
        raise NotImplementedError

    def concat(self, *others: StreamLike[T]) -> Streamable[T]:
        raise NotImplementedError

    def merge(self, *others: StreamLike[T]) -> Streamable[T]:
        raise NotImplementedError


class Streamables:
    @staticmethod
    def from_iterable[T](
        iterable: Iterable[T] | AsyncIterable[T] | Awaitable[Iterable[T] | AsyncIterable[T]],
    ) -> Streamable[T]:
        return _StreamableImpl(iterable)
```

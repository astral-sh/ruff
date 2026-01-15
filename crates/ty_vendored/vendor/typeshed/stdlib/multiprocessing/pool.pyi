from collections.abc import Callable, Iterable, Mapping
from multiprocessing.context import DefaultContext, Process
from types import GenericAlias, TracebackType
from typing import Any, Final, Generic, TypeVar
from typing_extensions import Self

__all__ = ["Pool", "ThreadPool"]

_S = TypeVar("_S")
_T = TypeVar("_T")

class ApplyResult(Generic[_T]):
    def __init__(
        self, pool: Pool, callback: Callable[[_T], object] | None, error_callback: Callable[[BaseException], object] | None
    ) -> None: ...
    def get(self, timeout: float | None = None) -> _T: ...
    def wait(self, timeout: float | None = None) -> None: ...
    def ready(self) -> bool: ...
    def successful(self) -> bool: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

# alias created during issue #17805
AsyncResult = ApplyResult

class MapResult(ApplyResult[list[_T]]):
    def __init__(
        self,
        pool: Pool,
        chunksize: int,
        length: int,
        callback: Callable[[list[_T]], object] | None,
        error_callback: Callable[[BaseException], object] | None,
    ) -> None: ...

class IMapIterator(Generic[_T]):
    def __init__(self, pool: Pool) -> None: ...
    def __iter__(self) -> Self: ...
    def next(self, timeout: float | None = None) -> _T: ...
    def __next__(self, timeout: float | None = None) -> _T: ...

class IMapUnorderedIterator(IMapIterator[_T]): ...

class Pool:
    """
    Class which supports an async version of applying functions to arguments.
    """

    def __init__(
        self,
        processes: int | None = None,
        initializer: Callable[..., object] | None = None,
        initargs: Iterable[Any] = (),
        maxtasksperchild: int | None = None,
        context: Any | None = None,
    ) -> None: ...
    @staticmethod
    def Process(ctx: DefaultContext, *args: Any, **kwds: Any) -> Process: ...
    def apply(self, func: Callable[..., _T], args: Iterable[Any] = (), kwds: Mapping[str, Any] = {}) -> _T:
        """
        Equivalent of `func(*args, **kwds)`.
        Pool must be running.
        """

    def apply_async(
        self,
        func: Callable[..., _T],
        args: Iterable[Any] = (),
        kwds: Mapping[str, Any] = {},
        callback: Callable[[_T], object] | None = None,
        error_callback: Callable[[BaseException], object] | None = None,
    ) -> AsyncResult[_T]:
        """
        Asynchronous version of `apply()` method.
        """

    def map(self, func: Callable[[_S], _T], iterable: Iterable[_S], chunksize: int | None = None) -> list[_T]:
        """
        Apply `func` to each element in `iterable`, collecting the results
        in a list that is returned.
        """

    def map_async(
        self,
        func: Callable[[_S], _T],
        iterable: Iterable[_S],
        chunksize: int | None = None,
        callback: Callable[[list[_T]], object] | None = None,
        error_callback: Callable[[BaseException], object] | None = None,
    ) -> MapResult[_T]:
        """
        Asynchronous version of `map()` method.
        """

    def imap(self, func: Callable[[_S], _T], iterable: Iterable[_S], chunksize: int | None = 1) -> IMapIterator[_T]:
        """
        Equivalent of `map()` -- can be MUCH slower than `Pool.map()`.
        """

    def imap_unordered(self, func: Callable[[_S], _T], iterable: Iterable[_S], chunksize: int | None = 1) -> IMapIterator[_T]:
        """
        Like `imap()` method but ordering of results is arbitrary.
        """

    def starmap(self, func: Callable[..., _T], iterable: Iterable[Iterable[Any]], chunksize: int | None = None) -> list[_T]:
        """
        Like `map()` method but the elements of the `iterable` are expected to
        be iterables as well and will be unpacked as arguments. Hence
        `func` and (a, b) becomes func(a, b).
        """

    def starmap_async(
        self,
        func: Callable[..., _T],
        iterable: Iterable[Iterable[Any]],
        chunksize: int | None = None,
        callback: Callable[[list[_T]], object] | None = None,
        error_callback: Callable[[BaseException], object] | None = None,
    ) -> AsyncResult[list[_T]]:
        """
        Asynchronous version of `starmap()` method.
        """

    def close(self) -> None: ...
    def terminate(self) -> None: ...
    def join(self) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> None: ...
    def __del__(self) -> None: ...

class ThreadPool(Pool):
    def __init__(
        self, processes: int | None = None, initializer: Callable[..., object] | None = None, initargs: Iterable[Any] = ()
    ) -> None: ...

# undocumented
INIT: Final = "INIT"
RUN: Final = "RUN"
CLOSE: Final = "CLOSE"
TERMINATE: Final = "TERMINATE"

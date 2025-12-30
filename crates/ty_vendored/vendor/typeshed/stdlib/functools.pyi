"""functools.py - Tools for working with functions and callable objects"""

import sys
import types
from _typeshed import SupportsAllComparisons, SupportsItems
from collections.abc import Callable, Hashable, Iterable, Sized
from types import GenericAlias
from typing import Any, Final, Generic, Literal, NamedTuple, TypedDict, TypeVar, final, overload, type_check_only
from typing_extensions import ParamSpec, Self, TypeAlias, disjoint_base

__all__ = [
    "update_wrapper",
    "wraps",
    "WRAPPER_ASSIGNMENTS",
    "WRAPPER_UPDATES",
    "total_ordering",
    "cmp_to_key",
    "lru_cache",
    "reduce",
    "partial",
    "partialmethod",
    "singledispatch",
    "cached_property",
    "singledispatchmethod",
    "cache",
]

_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)
_S = TypeVar("_S")
_PWrapped = ParamSpec("_PWrapped")
_RWrapped = TypeVar("_RWrapped")
_PWrapper = ParamSpec("_PWrapper")
_RWrapper = TypeVar("_RWrapper")

if sys.version_info >= (3, 14):
    @overload
    def reduce(function: Callable[[_T, _S], _T], iterable: Iterable[_S], /, initial: _T) -> _T:
        """Apply a function of two arguments cumulatively to the items of an iterable, from left to right.

        This effectively reduces the iterable to a single value.  If initial is present,
        it is placed before the items of the iterable in the calculation, and serves as
        a default when the iterable is empty.

        For example, reduce(lambda x, y: x+y, [1, 2, 3, 4, 5])
        calculates ((((1 + 2) + 3) + 4) + 5).
        """

else:
    @overload
    def reduce(function: Callable[[_T, _S], _T], iterable: Iterable[_S], initial: _T, /) -> _T:
        """reduce(function, iterable[, initial], /) -> value

        Apply a function of two arguments cumulatively to the items of an iterable, from left to right.

        This effectively reduces the iterable to a single value.  If initial is present,
        it is placed before the items of the iterable in the calculation, and serves as
        a default when the iterable is empty.

        For example, reduce(lambda x, y: x+y, [1, 2, 3, 4, 5])
        calculates ((((1 + 2) + 3) + 4) + 5).
        """

@overload
def reduce(function: Callable[[_T, _T], _T], iterable: Iterable[_T], /) -> _T:
    """Apply a function of two arguments cumulatively to the items of an iterable, from left to right.

    This effectively reduces the iterable to a single value.  If initial is present,
    it is placed before the items of the iterable in the calculation, and serves as
    a default when the iterable is empty.

    For example, reduce(lambda x, y: x+y, [1, 2, 3, 4, 5])
    calculates ((((1 + 2) + 3) + 4) + 5).
    """

class _CacheInfo(NamedTuple):
    """CacheInfo(hits, misses, maxsize, currsize)"""

    hits: int
    misses: int
    maxsize: int | None
    currsize: int

@type_check_only
class _CacheParameters(TypedDict):
    maxsize: int
    typed: bool

@final
class _lru_cache_wrapper(Generic[_T]):
    """Create a cached callable that wraps another function.

    user_function:      the function being cached

    maxsize:  0         for no caching
              None      for unlimited cache size
              n         for a bounded cache

    typed:    False     cache f(3) and f(3.0) as identical calls
              True      cache f(3) and f(3.0) as distinct calls

    cache_info_type:    namedtuple class with the fields:
                            hits misses currsize maxsize
    """

    __wrapped__: Callable[..., _T]
    def __call__(self, *args: Hashable, **kwargs: Hashable) -> _T:
        """Call self as a function."""

    def cache_info(self) -> _CacheInfo:
        """Report cache statistics"""

    def cache_clear(self) -> None:
        """Clear the cache and cache statistics"""

    def cache_parameters(self) -> _CacheParameters: ...
    def __copy__(self) -> _lru_cache_wrapper[_T]: ...
    def __deepcopy__(self, memo: Any, /) -> _lru_cache_wrapper[_T]: ...

@overload
def lru_cache(maxsize: int | None = 128, typed: bool = False) -> Callable[[Callable[..., _T]], _lru_cache_wrapper[_T]]:
    """Least-recently-used cache decorator.

    If *maxsize* is set to None, the LRU features are disabled and the cache
    can grow without bound.

    If *typed* is True, arguments of different types will be cached separately.
    For example, f(decimal.Decimal("3.0")) and f(3.0) will be treated as
    distinct calls with distinct results. Some types such as str and int may
    be cached separately even when typed is false.

    Arguments to the cached function must be hashable.

    View the cache statistics named tuple (hits, misses, maxsize, currsize)
    with f.cache_info().  Clear the cache and statistics with f.cache_clear().
    Access the underlying function with f.__wrapped__.

    See:  https://en.wikipedia.org/wiki/Cache_replacement_policies#Least_recently_used_(LRU)

    """

@overload
def lru_cache(maxsize: Callable[..., _T], typed: bool = False) -> _lru_cache_wrapper[_T]: ...

if sys.version_info >= (3, 14):
    WRAPPER_ASSIGNMENTS: Final[
        tuple[
            Literal["__module__"],
            Literal["__name__"],
            Literal["__qualname__"],
            Literal["__doc__"],
            Literal["__annotate__"],
            Literal["__type_params__"],
        ]
    ]
elif sys.version_info >= (3, 12):
    WRAPPER_ASSIGNMENTS: Final[
        tuple[
            Literal["__module__"],
            Literal["__name__"],
            Literal["__qualname__"],
            Literal["__doc__"],
            Literal["__annotations__"],
            Literal["__type_params__"],
        ]
    ]
else:
    WRAPPER_ASSIGNMENTS: Final[
        tuple[Literal["__module__"], Literal["__name__"], Literal["__qualname__"], Literal["__doc__"], Literal["__annotations__"]]
    ]

WRAPPER_UPDATES: Final[tuple[Literal["__dict__"]]]

@type_check_only
class _Wrapped(Generic[_PWrapped, _RWrapped, _PWrapper, _RWrapper]):
    __wrapped__: Callable[_PWrapped, _RWrapped]
    def __call__(self, *args: _PWrapper.args, **kwargs: _PWrapper.kwargs) -> _RWrapper: ...
    # as with ``Callable``, we'll assume that these attributes exist
    __name__: str
    __qualname__: str

@type_check_only
class _Wrapper(Generic[_PWrapped, _RWrapped]):
    def __call__(self, f: Callable[_PWrapper, _RWrapper]) -> _Wrapped[_PWrapped, _RWrapped, _PWrapper, _RWrapper]: ...

if sys.version_info >= (3, 14):
    def update_wrapper(
        wrapper: Callable[_PWrapper, _RWrapper],
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotate__", "__type_params__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapped[_PWrapped, _RWrapped, _PWrapper, _RWrapper]:
        """Update a wrapper function to look like the wrapped function

        wrapper is the function to be updated
        wrapped is the original function
        assigned is a tuple naming the attributes assigned directly
        from the wrapped function to the wrapper function (defaults to
        functools.WRAPPER_ASSIGNMENTS)
        updated is a tuple naming the attributes of the wrapper that
        are updated with the corresponding attribute from the wrapped
        function (defaults to functools.WRAPPER_UPDATES)
        """

    def wraps(
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotate__", "__type_params__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapper[_PWrapped, _RWrapped]:
        """Decorator factory to apply update_wrapper() to a wrapper function

        Returns a decorator that invokes update_wrapper() with the decorated
        function as the wrapper argument and the arguments to wraps() as the
        remaining arguments. Default arguments are as for update_wrapper().
        This is a convenience function to simplify applying partial() to
        update_wrapper().
        """

elif sys.version_info >= (3, 12):
    def update_wrapper(
        wrapper: Callable[_PWrapper, _RWrapper],
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotations__", "__type_params__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapped[_PWrapped, _RWrapped, _PWrapper, _RWrapper]:
        """Update a wrapper function to look like the wrapped function

        wrapper is the function to be updated
        wrapped is the original function
        assigned is a tuple naming the attributes assigned directly
        from the wrapped function to the wrapper function (defaults to
        functools.WRAPPER_ASSIGNMENTS)
        updated is a tuple naming the attributes of the wrapper that
        are updated with the corresponding attribute from the wrapped
        function (defaults to functools.WRAPPER_UPDATES)
        """

    def wraps(
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotations__", "__type_params__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapper[_PWrapped, _RWrapped]:
        """Decorator factory to apply update_wrapper() to a wrapper function

        Returns a decorator that invokes update_wrapper() with the decorated
        function as the wrapper argument and the arguments to wraps() as the
        remaining arguments. Default arguments are as for update_wrapper().
        This is a convenience function to simplify applying partial() to
        update_wrapper().
        """

else:
    def update_wrapper(
        wrapper: Callable[_PWrapper, _RWrapper],
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotations__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapped[_PWrapped, _RWrapped, _PWrapper, _RWrapper]:
        """Update a wrapper function to look like the wrapped function

        wrapper is the function to be updated
        wrapped is the original function
        assigned is a tuple naming the attributes assigned directly
        from the wrapped function to the wrapper function (defaults to
        functools.WRAPPER_ASSIGNMENTS)
        updated is a tuple naming the attributes of the wrapper that
        are updated with the corresponding attribute from the wrapped
        function (defaults to functools.WRAPPER_UPDATES)
        """

    def wraps(
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotations__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapper[_PWrapped, _RWrapped]:
        """Decorator factory to apply update_wrapper() to a wrapper function

        Returns a decorator that invokes update_wrapper() with the decorated
        function as the wrapper argument and the arguments to wraps() as the
        remaining arguments. Default arguments are as for update_wrapper().
        This is a convenience function to simplify applying partial() to
        update_wrapper().
        """

def total_ordering(cls: type[_T]) -> type[_T]:
    """Class decorator that fills in missing ordering methods"""

def cmp_to_key(mycmp: Callable[[_T, _T], int]) -> Callable[[_T], SupportsAllComparisons]:
    """Convert a cmp= function into a key= function.

    mycmp
      Function that compares two objects.
    """

@disjoint_base
class partial(Generic[_T]):
    """Create a new function with partial application of the given arguments
    and keywords.
    """

    @property
    def func(self) -> Callable[..., _T]:
        """function object to use in future partial calls"""

    @property
    def args(self) -> tuple[Any, ...]:
        """tuple of arguments to future partial calls"""

    @property
    def keywords(self) -> dict[str, Any]:
        """dictionary of keyword arguments to future partial calls"""

    def __new__(cls, func: Callable[..., _T], /, *args: Any, **kwargs: Any) -> Self: ...
    def __call__(self, /, *args: Any, **kwargs: Any) -> _T:
        """Call self as a function."""

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

# With protocols, this could change into a generic protocol that defines __get__ and returns _T
_Descriptor: TypeAlias = Any

class partialmethod(Generic[_T]):
    """Method descriptor with partial application of the given arguments
    and keywords.

    Supports wrapping existing descriptors and handles non-descriptor
    callables as instance methods.
    """

    func: Callable[..., _T] | _Descriptor
    args: tuple[Any, ...]
    keywords: dict[str, Any]
    if sys.version_info >= (3, 14):
        @overload
        def __new__(self, func: Callable[..., _T], /, *args: Any, **keywords: Any) -> Self: ...
        @overload
        def __new__(self, func: _Descriptor, /, *args: Any, **keywords: Any) -> Self: ...
    else:
        @overload
        def __init__(self, func: Callable[..., _T], /, *args: Any, **keywords: Any) -> None: ...
        @overload
        def __init__(self, func: _Descriptor, /, *args: Any, **keywords: Any) -> None: ...

    def __get__(self, obj: Any, cls: type[Any] | None = None) -> Callable[..., _T]: ...
    @property
    def __isabstractmethod__(self) -> bool: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

if sys.version_info >= (3, 11):
    _RegType: TypeAlias = type[Any] | types.UnionType
else:
    _RegType: TypeAlias = type[Any]

@type_check_only
class _SingleDispatchCallable(Generic[_T]):
    registry: types.MappingProxyType[Any, Callable[..., _T]]
    def dispatch(self, cls: Any) -> Callable[..., _T]: ...
    # @fun.register(complex)
    # def _(arg, verbose=False): ...
    @overload
    def register(self, cls: _RegType, func: None = None) -> Callable[[Callable[..., _T]], Callable[..., _T]]: ...
    # @fun.register
    # def _(arg: int, verbose=False):
    @overload
    def register(self, cls: Callable[..., _T], func: None = None) -> Callable[..., _T]: ...
    # fun.register(int, lambda x: x)
    @overload
    def register(self, cls: _RegType, func: Callable[..., _T]) -> Callable[..., _T]: ...
    def _clear_cache(self) -> None: ...
    def __call__(self, /, *args: Any, **kwargs: Any) -> _T: ...

def singledispatch(func: Callable[..., _T]) -> _SingleDispatchCallable[_T]:
    """Single-dispatch generic function decorator.

    Transforms a function into a generic function, which can have different
    behaviours depending upon the type of its first argument. The decorated
    function acts as the default implementation, and additional
    implementations can be registered using the register() attribute of the
    generic function.
    """

class singledispatchmethod(Generic[_T]):
    """Single-dispatch generic method descriptor.

    Supports wrapping existing descriptors.
    """

    dispatcher: _SingleDispatchCallable[_T]
    func: Callable[..., _T]
    def __init__(self, func: Callable[..., _T]) -> None: ...
    @property
    def __isabstractmethod__(self) -> bool: ...
    @overload
    def register(self, cls: _RegType, method: None = None) -> Callable[[Callable[..., _T]], Callable[..., _T]]:
        """generic_method.register(cls, func) -> func

        Registers a new implementation for the given *cls* on a *generic_method*.
        """

    @overload
    def register(self, cls: Callable[..., _T], method: None = None) -> Callable[..., _T]: ...
    @overload
    def register(self, cls: _RegType, method: Callable[..., _T]) -> Callable[..., _T]: ...
    def __get__(self, obj: _S, cls: type[_S] | None = None) -> Callable[..., _T]: ...

class cached_property(Generic[_T_co]):
    func: Callable[[Any], _T_co]
    attrname: str | None
    def __init__(self, func: Callable[[Any], _T_co]) -> None: ...
    @overload
    def __get__(self, instance: None, owner: type[Any] | None = None) -> Self: ...
    @overload
    def __get__(self, instance: object, owner: type[Any] | None = None) -> _T_co: ...
    def __set_name__(self, owner: type[Any], name: str) -> None: ...
    # __set__ is not defined at runtime, but @cached_property is designed to be settable
    def __set__(self, instance: object, value: _T_co) -> None: ...  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

def cache(user_function: Callable[..., _T], /) -> _lru_cache_wrapper[_T]:
    """Simple lightweight unbounded cache.  Sometimes called "memoize"."""

def _make_key(
    args: tuple[Hashable, ...],
    kwds: SupportsItems[Any, Any],
    typed: bool,
    kwd_mark: tuple[object, ...] = ...,
    fasttypes: set[type] = ...,
    tuple: type = ...,
    type: Any = ...,
    len: Callable[[Sized], int] = ...,
) -> Hashable:
    """Make a cache key from optionally typed positional and keyword arguments

    The key is constructed in a way that is flat as possible rather than
    as a nested structure that would take more memory.

    If there is only a single argument and its data type is known to cache
    its hash value, then that argument is returned without a wrapper.  This
    saves space and improves lookup speed.

    """

if sys.version_info >= (3, 14):
    @final
    class _PlaceholderType:
        """The type of the Placeholder singleton.

        Used as a placeholder for partial arguments.
        """

    Placeholder: Final[_PlaceholderType]

    __all__ += ["Placeholder"]

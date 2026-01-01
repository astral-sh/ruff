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
    def reduce(function: Callable[[_T, _S], _T], iterable: Iterable[_S], /, initial: _T) -> _T: ...

else:
    @overload
    def reduce(function: Callable[[_T, _S], _T], iterable: Iterable[_S], initial: _T, /) -> _T: ...

@overload
def reduce(function: Callable[[_T, _T], _T], iterable: Iterable[_T], /) -> _T: ...

class _CacheInfo(NamedTuple):
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
    __wrapped__: Callable[..., _T]
    def __call__(self, *args: Hashable, **kwargs: Hashable) -> _T: ...
    def cache_info(self) -> _CacheInfo: ...
    def cache_clear(self) -> None: ...
    def cache_parameters(self) -> _CacheParameters: ...
    def __copy__(self) -> _lru_cache_wrapper[_T]: ...
    def __deepcopy__(self, memo: Any, /) -> _lru_cache_wrapper[_T]: ...

@overload
def lru_cache(maxsize: int | None = 128, typed: bool = False) -> Callable[[Callable[..., _T]], _lru_cache_wrapper[_T]]: ...
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
    ) -> _Wrapped[_PWrapped, _RWrapped, _PWrapper, _RWrapper]: ...
    def wraps(
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotate__", "__type_params__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapper[_PWrapped, _RWrapped]: ...

elif sys.version_info >= (3, 12):
    def update_wrapper(
        wrapper: Callable[_PWrapper, _RWrapper],
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotations__", "__type_params__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapped[_PWrapped, _RWrapped, _PWrapper, _RWrapper]: ...
    def wraps(
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotations__", "__type_params__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapper[_PWrapped, _RWrapped]: ...

else:
    def update_wrapper(
        wrapper: Callable[_PWrapper, _RWrapper],
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotations__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapped[_PWrapped, _RWrapped, _PWrapper, _RWrapper]: ...
    def wraps(
        wrapped: Callable[_PWrapped, _RWrapped],
        assigned: Iterable[str] = ("__module__", "__name__", "__qualname__", "__doc__", "__annotations__"),
        updated: Iterable[str] = ("__dict__",),
    ) -> _Wrapper[_PWrapped, _RWrapped]: ...

def total_ordering(cls: type[_T]) -> type[_T]: ...
def cmp_to_key(mycmp: Callable[[_T, _T], int]) -> Callable[[_T], SupportsAllComparisons]: ...
@disjoint_base
class partial(Generic[_T]):
    @property
    def func(self) -> Callable[..., _T]: ...
    @property
    def args(self) -> tuple[Any, ...]: ...
    @property
    def keywords(self) -> dict[str, Any]: ...
    def __new__(cls, func: Callable[..., _T], /, *args: Any, **kwargs: Any) -> Self: ...
    def __call__(self, /, *args: Any, **kwargs: Any) -> _T: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

# With protocols, this could change into a generic protocol that defines __get__ and returns _T
_Descriptor: TypeAlias = Any

class partialmethod(Generic[_T]):
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
    def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

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

def singledispatch(func: Callable[..., _T]) -> _SingleDispatchCallable[_T]: ...

class singledispatchmethod(Generic[_T]):
    dispatcher: _SingleDispatchCallable[_T]
    func: Callable[..., _T]
    def __init__(self, func: Callable[..., _T]) -> None: ...
    @property
    def __isabstractmethod__(self) -> bool: ...
    @overload
    def register(self, cls: _RegType, method: None = None) -> Callable[[Callable[..., _T]], Callable[..., _T]]: ...
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
    def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

def cache(user_function: Callable[..., _T], /) -> _lru_cache_wrapper[_T]: ...
def _make_key(
    args: tuple[Hashable, ...],
    kwds: SupportsItems[Any, Any],
    typed: bool,
    kwd_mark: tuple[object, ...] = ...,
    fasttypes: set[type] = ...,
    tuple: type = ...,
    type: Any = ...,
    len: Callable[[Sized], int] = ...,
) -> Hashable: ...

if sys.version_info >= (3, 14):
    @final
    class _PlaceholderType: ...

    Placeholder: Final[_PlaceholderType]

    __all__ += ["Placeholder"]

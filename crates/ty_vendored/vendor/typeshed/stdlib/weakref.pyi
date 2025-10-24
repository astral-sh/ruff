"""Weak reference support for Python.

This module is an implementation of PEP 205:

https://peps.python.org/pep-0205/
"""

from _typeshed import SupportsKeysAndGetItem
from _weakref import getweakrefcount as getweakrefcount, getweakrefs as getweakrefs, proxy as proxy
from _weakrefset import WeakSet as WeakSet
from collections.abc import Callable, Iterable, Iterator, Mapping, MutableMapping
from types import GenericAlias
from typing import Any, ClassVar, Generic, TypeVar, final, overload
from typing_extensions import ParamSpec, Self, disjoint_base

__all__ = [
    "ref",
    "proxy",
    "getweakrefcount",
    "getweakrefs",
    "WeakKeyDictionary",
    "ReferenceType",
    "ProxyType",
    "CallableProxyType",
    "ProxyTypes",
    "WeakValueDictionary",
    "WeakSet",
    "WeakMethod",
    "finalize",
]

_T = TypeVar("_T")
_T1 = TypeVar("_T1")
_T2 = TypeVar("_T2")
_KT = TypeVar("_KT")
_VT = TypeVar("_VT")
_CallableT = TypeVar("_CallableT", bound=Callable[..., Any])
_P = ParamSpec("_P")

ProxyTypes: tuple[type[Any], ...]

# These classes are implemented in C and imported from _weakref at runtime. However,
# they consider themselves to live in the weakref module for sys.version_info >= (3, 11),
# so defining their stubs here means we match their __module__ value.
# Prior to 3.11 they did not declare a module for themselves and ended up looking like they
# came from the builtin module at runtime, which was just wrong, and we won't attempt to
# duplicate that.

@final
class CallableProxyType(Generic[_CallableT]):  # "weakcallableproxy"
    def __eq__(self, value: object, /) -> bool: ...
    def __getattr__(self, attr: str) -> Any: ...
    __call__: _CallableT
    __hash__: ClassVar[None]  # type: ignore[assignment]

@final
class ProxyType(Generic[_T]):  # "weakproxy"
    def __eq__(self, value: object, /) -> bool: ...
    def __getattr__(self, attr: str) -> Any: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

@disjoint_base
class ReferenceType(Generic[_T]):  # "weakref"
    __callback__: Callable[[Self], Any]
    def __new__(cls, o: _T, callback: Callable[[Self], Any] | None = ..., /) -> Self: ...
    def __call__(self) -> _T | None:
        """Call self as a function."""

    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

ref = ReferenceType

# everything below here is implemented in weakref.py

class WeakMethod(ref[_CallableT]):
    """
    A custom `weakref.ref` subclass which simulates a weak reference to
    a bound method, working around the lifetime problem of bound methods.
    """

    __slots__ = ("_func_ref", "_meth_type", "_alive", "__weakref__")
    def __new__(cls, meth: _CallableT, callback: Callable[[Self], Any] | None = None) -> Self: ...
    def __call__(self) -> _CallableT | None: ...
    def __eq__(self, other: object) -> bool: ...
    def __ne__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class WeakValueDictionary(MutableMapping[_KT, _VT]):
    """Mapping class that references values weakly.

    Entries in the dictionary will be discarded when no strong
    reference to the value exists anymore
    """

    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(
        self: WeakValueDictionary[_KT, _VT],  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        other: Mapping[_KT, _VT] | Iterable[tuple[_KT, _VT]],
        /,
    ) -> None: ...
    @overload
    def __init__(
        self: WeakValueDictionary[str, _VT],  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        other: Mapping[str, _VT] | Iterable[tuple[str, _VT]] = (),
        /,
        **kwargs: _VT,
    ) -> None: ...
    def __len__(self) -> int: ...
    def __getitem__(self, key: _KT) -> _VT: ...
    def __setitem__(self, key: _KT, value: _VT) -> None: ...
    def __delitem__(self, key: _KT) -> None: ...
    def __contains__(self, key: object) -> bool: ...
    def __iter__(self) -> Iterator[_KT]: ...
    def copy(self) -> WeakValueDictionary[_KT, _VT]: ...
    __copy__ = copy
    def __deepcopy__(self, memo: Any) -> Self: ...
    @overload
    def get(self, key: _KT, default: None = None) -> _VT | None: ...
    @overload
    def get(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def get(self, key: _KT, default: _T) -> _VT | _T: ...
    # These are incompatible with Mapping
    def keys(self) -> Iterator[_KT]: ...  # type: ignore[override]
    def values(self) -> Iterator[_VT]: ...  # type: ignore[override]
    def items(self) -> Iterator[tuple[_KT, _VT]]: ...  # type: ignore[override]
    def itervaluerefs(self) -> Iterator[KeyedRef[_KT, _VT]]:
        """Return an iterator that yields the weak references to the values.

        The references are not guaranteed to be 'live' at the time
        they are used, so the result of calling the references needs
        to be checked before being used.  This can be used to avoid
        creating references that will cause the garbage collector to
        keep the values around longer than needed.

        """

    def valuerefs(self) -> list[KeyedRef[_KT, _VT]]:
        """Return a list of weak references to the values.

        The references are not guaranteed to be 'live' at the time
        they are used, so the result of calling the references needs
        to be checked before being used.  This can be used to avoid
        creating references that will cause the garbage collector to
        keep the values around longer than needed.

        """

    def setdefault(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def pop(self, key: _KT) -> _VT: ...
    @overload
    def pop(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def pop(self, key: _KT, default: _T) -> _VT | _T: ...
    @overload
    def update(self, other: SupportsKeysAndGetItem[_KT, _VT], /, **kwargs: _VT) -> None: ...
    @overload
    def update(self, other: Iterable[tuple[_KT, _VT]], /, **kwargs: _VT) -> None: ...
    @overload
    def update(self, other: None = None, /, **kwargs: _VT) -> None: ...
    def __or__(self, other: Mapping[_T1, _T2]) -> WeakValueDictionary[_KT | _T1, _VT | _T2]: ...
    def __ror__(self, other: Mapping[_T1, _T2]) -> WeakValueDictionary[_KT | _T1, _VT | _T2]: ...
    # WeakValueDictionary.__ior__ should be kept roughly in line with MutableMapping.update()
    @overload  # type: ignore[misc]
    def __ior__(self, other: SupportsKeysAndGetItem[_KT, _VT]) -> Self: ...
    @overload
    def __ior__(self, other: Iterable[tuple[_KT, _VT]]) -> Self: ...

class KeyedRef(ref[_T], Generic[_KT, _T]):
    """Specialized reference that includes a key corresponding to the value.

    This is used in the WeakValueDictionary to avoid having to create
    a function object for each key stored in the mapping.  A shared
    callback object can use the 'key' attribute of a KeyedRef instead
    of getting a reference to the key from an enclosing scope.

    """

    __slots__ = ("key",)
    key: _KT
    def __new__(type, ob: _T, callback: Callable[[Self], Any], key: _KT) -> Self: ...
    def __init__(self, ob: _T, callback: Callable[[Self], Any], key: _KT) -> None: ...

class WeakKeyDictionary(MutableMapping[_KT, _VT]):
    """Mapping class that references keys weakly.

    Entries in the dictionary will be discarded when there is no
    longer a strong reference to the key. This can be used to
    associate additional data with an object owned by other parts of
    an application without adding attributes to those objects. This
    can be especially useful with objects that override attribute
    accesses.
    """

    @overload
    def __init__(self, dict: None = None) -> None: ...
    @overload
    def __init__(self, dict: Mapping[_KT, _VT] | Iterable[tuple[_KT, _VT]]) -> None: ...
    def __len__(self) -> int: ...
    def __getitem__(self, key: _KT) -> _VT: ...
    def __setitem__(self, key: _KT, value: _VT) -> None: ...
    def __delitem__(self, key: _KT) -> None: ...
    def __contains__(self, key: object) -> bool: ...
    def __iter__(self) -> Iterator[_KT]: ...
    def copy(self) -> WeakKeyDictionary[_KT, _VT]: ...
    __copy__ = copy
    def __deepcopy__(self, memo: Any) -> Self: ...
    @overload
    def get(self, key: _KT, default: None = None) -> _VT | None: ...
    @overload
    def get(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def get(self, key: _KT, default: _T) -> _VT | _T: ...
    # These are incompatible with Mapping
    def keys(self) -> Iterator[_KT]: ...  # type: ignore[override]
    def values(self) -> Iterator[_VT]: ...  # type: ignore[override]
    def items(self) -> Iterator[tuple[_KT, _VT]]: ...  # type: ignore[override]
    def keyrefs(self) -> list[ref[_KT]]:
        """Return a list of weak references to the keys.

        The references are not guaranteed to be 'live' at the time
        they are used, so the result of calling the references needs
        to be checked before being used.  This can be used to avoid
        creating references that will cause the garbage collector to
        keep the keys around longer than needed.

        """
    # Keep WeakKeyDictionary.setdefault in line with MutableMapping.setdefault, modulo positional-only differences
    @overload
    def setdefault(self: WeakKeyDictionary[_KT, _VT | None], key: _KT, default: None = None) -> _VT: ...
    @overload
    def setdefault(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def pop(self, key: _KT) -> _VT: ...
    @overload
    def pop(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def pop(self, key: _KT, default: _T) -> _VT | _T: ...
    @overload
    def update(self, dict: SupportsKeysAndGetItem[_KT, _VT], /, **kwargs: _VT) -> None: ...
    @overload
    def update(self, dict: Iterable[tuple[_KT, _VT]], /, **kwargs: _VT) -> None: ...
    @overload
    def update(self, dict: None = None, /, **kwargs: _VT) -> None: ...
    def __or__(self, other: Mapping[_T1, _T2]) -> WeakKeyDictionary[_KT | _T1, _VT | _T2]: ...
    def __ror__(self, other: Mapping[_T1, _T2]) -> WeakKeyDictionary[_KT | _T1, _VT | _T2]: ...
    # WeakKeyDictionary.__ior__ should be kept roughly in line with MutableMapping.update()
    @overload  # type: ignore[misc]
    def __ior__(self, other: SupportsKeysAndGetItem[_KT, _VT]) -> Self: ...
    @overload
    def __ior__(self, other: Iterable[tuple[_KT, _VT]]) -> Self: ...

class finalize(Generic[_P, _T]):
    """Class for finalization of weakrefable objects

    finalize(obj, func, *args, **kwargs) returns a callable finalizer
    object which will be called when obj is garbage collected. The
    first time the finalizer is called it evaluates func(*arg, **kwargs)
    and returns the result. After this the finalizer is dead, and
    calling it just returns None.

    When the program exits any remaining finalizers for which the
    atexit attribute is true will be run in reverse order of creation.
    By default atexit is true.
    """

    __slots__ = ()
    def __init__(self, obj: _T, func: Callable[_P, Any], /, *args: _P.args, **kwargs: _P.kwargs) -> None: ...
    def __call__(self, _: Any = None) -> Any | None:
        """If alive then mark as dead and return func(*args, **kwargs);
        otherwise return None
        """

    def detach(self) -> tuple[_T, Callable[_P, Any], tuple[Any, ...], dict[str, Any]] | None:
        """If alive then mark as dead and return (obj, func, args, kwargs);
        otherwise return None
        """

    def peek(self) -> tuple[_T, Callable[_P, Any], tuple[Any, ...], dict[str, Any]] | None:
        """If alive then return (obj, func, args, kwargs);
        otherwise return None
        """

    @property
    def alive(self) -> bool:
        """Whether finalizer is alive"""
    atexit: bool

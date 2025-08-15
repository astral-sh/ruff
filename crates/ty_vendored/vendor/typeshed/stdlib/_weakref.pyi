from collections.abc import Callable
from typing import Any, TypeVar, overload
from weakref import CallableProxyType as CallableProxyType, ProxyType as ProxyType, ReferenceType as ReferenceType, ref as ref

_C = TypeVar("_C", bound=Callable[..., Any])
_T = TypeVar("_T")

def getweakrefcount(object: Any, /) -> int: ...
def getweakrefs(object: Any, /) -> list[Any]: ...

# Return CallableProxyType if object is callable, ProxyType otherwise
@overload
def proxy(object: _C, callback: Callable[[_C], Any] | None = None, /) -> CallableProxyType[_C]: ...
@overload
def proxy(object: _T, callback: Callable[[_T], Any] | None = None, /) -> Any: ...

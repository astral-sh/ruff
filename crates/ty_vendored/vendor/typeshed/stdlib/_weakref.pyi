"""Weak-reference support module."""

from collections.abc import Callable
from typing import Any, TypeVar, overload
from weakref import CallableProxyType as CallableProxyType, ProxyType as ProxyType, ReferenceType as ReferenceType, ref as ref

_C = TypeVar("_C", bound=Callable[..., Any])
_T = TypeVar("_T")

def getweakrefcount(object: Any, /) -> int:
    """Return the number of weak references to 'object'."""

def getweakrefs(object: Any, /) -> list[Any]:
    """Return a list of all weak reference objects pointing to 'object'."""

# Return CallableProxyType if object is callable, ProxyType otherwise
@overload
def proxy(object: _C, callback: Callable[[_C], Any] | None = None, /) -> CallableProxyType[_C]:
    """Create a proxy object that weakly references 'object'.

    'callback', if given, is called with a reference to the
    proxy when 'object' is about to be finalized.
    """

@overload
def proxy(object: _T, callback: Callable[[_T], Any] | None = None, /) -> Any: ...

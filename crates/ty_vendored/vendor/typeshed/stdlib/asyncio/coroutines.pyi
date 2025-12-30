import sys
from collections.abc import Awaitable, Callable, Coroutine
from typing import Any, TypeVar, overload
from typing_extensions import ParamSpec, TypeGuard, TypeIs, deprecated

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.version_info >= (3, 11):
    __all__ = ("iscoroutinefunction", "iscoroutine")
else:
    __all__ = ("coroutine", "iscoroutinefunction", "iscoroutine")

_T = TypeVar("_T")
_FunctionT = TypeVar("_FunctionT", bound=Callable[..., Any])
_P = ParamSpec("_P")

if sys.version_info < (3, 11):
    @deprecated("Deprecated since Python 3.8; removed in Python 3.11. Use `async def` instead.")
    def coroutine(func: _FunctionT) -> _FunctionT: ...

def iscoroutine(obj: object) -> TypeIs[Coroutine[Any, Any, Any]]: ...

if sys.version_info >= (3, 11):
    @overload
    @deprecated("Deprecated since Python 3.14. Use `inspect.iscoroutinefunction()` instead.")
    def iscoroutinefunction(func: Callable[..., Coroutine[Any, Any, Any]]) -> bool: ...
    @overload
    @deprecated("Deprecated since Python 3.14. Use `inspect.iscoroutinefunction()` instead.")
    def iscoroutinefunction(func: Callable[_P, Awaitable[_T]]) -> TypeGuard[Callable[_P, Coroutine[Any, Any, _T]]]: ...
    @overload
    @deprecated("Deprecated since Python 3.14. Use `inspect.iscoroutinefunction()` instead.")
    def iscoroutinefunction(func: Callable[_P, object]) -> TypeGuard[Callable[_P, Coroutine[Any, Any, Any]]]: ...
    @overload
    @deprecated("Deprecated since Python 3.14. Use `inspect.iscoroutinefunction()` instead.")
    def iscoroutinefunction(func: object) -> TypeGuard[Callable[..., Coroutine[Any, Any, Any]]]: ...

else:
    # Sometimes needed in Python < 3.11 due to the fact that it supports @coroutine
    # which was removed in 3.11 which the inspect version doesn't support.

    @overload
    def iscoroutinefunction(func: Callable[..., Coroutine[Any, Any, Any]]) -> bool: ...
    @overload
    def iscoroutinefunction(func: Callable[_P, Awaitable[_T]]) -> TypeGuard[Callable[_P, Coroutine[Any, Any, _T]]]: ...
    @overload
    def iscoroutinefunction(func: Callable[_P, object]) -> TypeGuard[Callable[_P, Coroutine[Any, Any, Any]]]: ...
    @overload
    def iscoroutinefunction(func: object) -> TypeGuard[Callable[..., Coroutine[Any, Any, Any]]]: ...

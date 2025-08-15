"""Context Variables"""

import sys
from collections.abc import Callable, Iterator, Mapping
from types import GenericAlias, TracebackType
from typing import Any, ClassVar, Generic, TypeVar, final, overload
from typing_extensions import ParamSpec, Self

_T = TypeVar("_T")
_D = TypeVar("_D")
_P = ParamSpec("_P")

@final
class ContextVar(Generic[_T]):
    @overload
    def __new__(cls, name: str) -> Self: ...
    @overload
    def __new__(cls, name: str, *, default: _T) -> Self: ...
    def __hash__(self) -> int: ...
    @property
    def name(self) -> str: ...
    @overload
    def get(self) -> _T:
        """Return a value for the context variable for the current context.

        If there is no value for the variable in the current context, the method will:
         * return the value of the default argument of the method, if provided; or
         * return the default value for the context variable, if it was created
           with one; or
         * raise a LookupError.
        """

    @overload
    def get(self, default: _T, /) -> _T: ...
    @overload
    def get(self, default: _D, /) -> _D | _T: ...
    def set(self, value: _T, /) -> Token[_T]:
        """Call to set a new value for the context variable in the current context.

        The required value argument is the new value for the context variable.

        Returns a Token object that can be used to restore the variable to its previous
        value via the `ContextVar.reset()` method.
        """

    def reset(self, token: Token[_T], /) -> None:
        """Reset the context variable.

        The variable is reset to the value it had before the `ContextVar.set()` that
        created the token was used.
        """

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

@final
class Token(Generic[_T]):
    @property
    def var(self) -> ContextVar[_T]: ...
    @property
    def old_value(self) -> Any: ...  # returns either _T or MISSING, but that's hard to express
    MISSING: ClassVar[object]
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""
    if sys.version_info >= (3, 14):
        def __enter__(self) -> Self:
            """Enter into Token context manager."""

        def __exit__(
            self, exc_type: type[BaseException] | None, exc_value: BaseException | None, traceback: TracebackType | None, /
        ) -> None:
            """Exit from Token context manager, restore the linked ContextVar."""

def copy_context() -> Context: ...

# It doesn't make sense to make this generic, because for most Contexts each ContextVar will have
# a different value.
@final
class Context(Mapping[ContextVar[Any], Any]):
    def __init__(self) -> None: ...
    @overload
    def get(self, key: ContextVar[_T], default: None = None, /) -> _T | None:
        """Return the value for `key` if `key` has the value in the context object.

        If `key` does not exist, return `default`. If `default` is not given,
        return None.
        """

    @overload
    def get(self, key: ContextVar[_T], default: _T, /) -> _T: ...
    @overload
    def get(self, key: ContextVar[_T], default: _D, /) -> _T | _D: ...
    def run(self, callable: Callable[_P, _T], *args: _P.args, **kwargs: _P.kwargs) -> _T: ...
    def copy(self) -> Context:
        """Return a shallow copy of the context object."""
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __getitem__(self, key: ContextVar[_T], /) -> _T:
        """Return self[key]."""

    def __iter__(self) -> Iterator[ContextVar[Any]]:
        """Implement iter(self)."""

    def __len__(self) -> int:
        """Return len(self)."""

    def __eq__(self, value: object, /) -> bool: ...

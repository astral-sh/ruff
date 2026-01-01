"""Helper to provide extensibility for pickle.

This is only useful to add pickle support for extension types defined in
C, not for instances of user-defined classes.
"""

from collections.abc import Callable, Hashable
from typing import Any, SupportsInt, TypeVar
from typing_extensions import TypeAlias

_T = TypeVar("_T")
_Reduce: TypeAlias = tuple[Callable[..., _T], tuple[Any, ...]] | tuple[Callable[..., _T], tuple[Any, ...], Any | None]

__all__ = ["pickle", "constructor", "add_extension", "remove_extension", "clear_extension_cache"]

def pickle(
    ob_type: type[_T],
    pickle_function: Callable[[_T], str | _Reduce[_T]],
    constructor_ob: Callable[[_Reduce[_T]], _T] | None = None,
) -> None: ...
def constructor(object: Callable[[_Reduce[_T]], _T]) -> None: ...
def add_extension(module: Hashable, name: Hashable, code: SupportsInt) -> None:
    """Register an extension code."""

def remove_extension(module: Hashable, name: Hashable, code: int) -> None:
    """Unregister an extension code.  For testing only."""

def clear_extension_cache() -> None: ...

_DispatchTableType: TypeAlias = dict[type, Callable[[Any], str | _Reduce[Any]]]  # imported by multiprocessing.reduction
dispatch_table: _DispatchTableType  # undocumented

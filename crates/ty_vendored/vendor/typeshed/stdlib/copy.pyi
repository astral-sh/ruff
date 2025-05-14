import sys
from typing import Any, Protocol, TypeVar
from typing_extensions import Self

__all__ = ["Error", "copy", "deepcopy"]

_T = TypeVar("_T")
_SR = TypeVar("_SR", bound=_SupportsReplace)

class _SupportsReplace(Protocol):
    # In reality doesn't support args, but there's no other great way to express this.
    def __replace__(self, *args: Any, **kwargs: Any) -> Self: ...

# None in CPython but non-None in Jython
PyStringMap: Any

# Note: memo and _nil are internal kwargs.
def deepcopy(x: _T, memo: dict[int, Any] | None = None, _nil: Any = []) -> _T: ...
def copy(x: _T) -> _T: ...

if sys.version_info >= (3, 13):
    __all__ += ["replace"]
    def replace(obj: _SR, /, **changes: Any) -> _SR: ...

class Error(Exception): ...

error = Error

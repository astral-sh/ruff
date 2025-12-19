import sys
from typing import Any, Protocol, TypeVar, type_check_only

__all__ = ["Error", "copy", "deepcopy"]

_T = TypeVar("_T")
_RT_co = TypeVar("_RT_co", covariant=True)

@type_check_only
class _SupportsReplace(Protocol[_RT_co]):
    # In reality doesn't support args, but there's no great way to express this.
    def __replace__(self, /, *_: Any, **changes: Any) -> _RT_co: ...

# None in CPython but non-None in Jython
PyStringMap: Any

# Note: memo and _nil are internal kwargs.
def deepcopy(x: _T, memo: dict[int, Any] | None = None, _nil: Any = []) -> _T: ...
def copy(x: _T) -> _T: ...

if sys.version_info >= (3, 13):
    __all__ += ["replace"]
    # The types accepted by `**changes` match those of `obj.__replace__`.
    def replace(obj: _SupportsReplace[_RT_co], /, **changes: Any) -> _RT_co: ...

class Error(Exception): ...

error = Error

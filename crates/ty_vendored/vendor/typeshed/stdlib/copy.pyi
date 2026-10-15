import sys
from typing import Any, Protocol, TypeVar, type_check_only
from typing_extensions import ParamSpec

__all__ = ["Error", "copy", "deepcopy"]

_T = TypeVar("_T")
_RT_co = TypeVar("_RT_co", covariant=True)
_P = ParamSpec("_P")

@type_check_only
class _SupportsReplace(Protocol[_P, _RT_co]):
    # In reality doesn't support args, but there's no great way to express this.
    def __replace__(self, /, *_: _P.args, **changes: _P.kwargs) -> _RT_co: ...

# None in CPython but non-None in Jython
PyStringMap: Any

def copy(x: _T) -> _T: ...

if sys.version_info >= (3, 15):
    def deepcopy(x: _T, memo: dict[int, Any] | None = None) -> _T: ...

else:
    # Note: memo and _nil are internal kwargs.
    def deepcopy(x: _T, memo: dict[int, Any] | None = None, _nil: Any = []) -> _T: ...

if sys.version_info >= (3, 13):
    __all__ += ["replace"]

    def replace(
        obj: _SupportsReplace[_P, _RT_co], /, *_: _P.args, **changes: _P.kwargs  # does not accept positional arguments at runtime
    ) -> _RT_co: ...

class Error(Exception): ...

error = Error

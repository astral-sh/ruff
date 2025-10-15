"""Generic (shallow and deep) copying operations.

Interface summary:

        import copy

        x = copy.copy(y)                # make a shallow copy of y
        x = copy.deepcopy(y)            # make a deep copy of y
        x = copy.replace(y, a=1, b=2)   # new object with fields replaced, as defined by `__replace__`

For module specific errors, copy.Error is raised.

The difference between shallow and deep copying is only relevant for
compound objects (objects that contain other objects, like lists or
class instances).

- A shallow copy constructs a new compound object and then (to the
  extent possible) inserts *the same objects* into it that the
  original contains.

- A deep copy constructs a new compound object and then, recursively,
  inserts *copies* into it of the objects found in the original.

Two problems often exist with deep copy operations that don't exist
with shallow copy operations:

 a) recursive objects (compound objects that, directly or indirectly,
    contain a reference to themselves) may cause a recursive loop

 b) because deep copy copies *everything* it may copy too much, e.g.
    administrative data structures that should be shared even between
    copies

Python's deep copy operation avoids these problems by:

 a) keeping a table of objects already copied during the current
    copying pass

 b) letting user-defined classes override the copying operation or the
    set of components copied

This version does not copy types like module, class, function, method,
nor stack trace, stack frame, nor file, socket, window, nor any
similar types.

Classes can use the same interfaces to control copying that they use
to control pickling: they can define methods called __getinitargs__(),
__getstate__() and __setstate__().  See the documentation for module
"pickle" for information on these methods.
"""

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
def deepcopy(x: _T, memo: dict[int, Any] | None = None, _nil: Any = []) -> _T:
    """Deep copy operation on arbitrary Python objects.

    See the module's __doc__ string for more info.
    """

def copy(x: _T) -> _T:
    """Shallow copy operation on arbitrary Python objects.

    See the module's __doc__ string for more info.
    """

if sys.version_info >= (3, 13):
    __all__ += ["replace"]
    # The types accepted by `**changes` match those of `obj.__replace__`.
    def replace(obj: _SupportsReplace[_RT_co], /, **changes: Any) -> _RT_co:
        """Return a new object replacing specified fields with new values.

        This is especially useful for immutable objects, like named tuples or
        frozen dataclasses.
        """

class Error(Exception): ...

error = Error

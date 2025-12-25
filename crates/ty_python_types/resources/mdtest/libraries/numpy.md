# numpy

```toml
[environment]
python-version = "3.14"
```

## numpy's `dtype`

numpy functions often accept a `dtype` parameter. For example, one of `np.array`'s overloads accepts
a `dtype` parameter of type `DTypeLike | None`. Here, we build up something that resembles numpy's
internals in order to model the type `DTypeLike`. Many details have been left out.

`mini_numpy.py`:

```py
from typing import TypeVar, Generic, Any, Protocol, TypeAlias, runtime_checkable, final
import builtins

_ItemT_co = TypeVar("_ItemT_co", default=Any, covariant=True)

class generic(Generic[_ItemT_co]):
    @property
    def dtype(self) -> _DTypeT_co:
        raise NotImplementedError

_BoolItemT_co = TypeVar("_BoolItemT_co", bound=builtins.bool, default=builtins.bool, covariant=True)

class bool(generic[_BoolItemT_co], Generic[_BoolItemT_co]): ...

@final
class object_(generic): ...

_ScalarT = TypeVar("_ScalarT", bound=generic)
_ScalarT_co = TypeVar("_ScalarT_co", bound=generic, default=Any, covariant=True)

@final
class dtype(Generic[_ScalarT_co]): ...

_DTypeT_co = TypeVar("_DTypeT_co", bound=dtype, default=dtype, covariant=True)

@runtime_checkable
class _SupportsDType(Protocol[_DTypeT_co]):
    @property
    def dtype(self) -> _DTypeT_co: ...

_DTypeLike: TypeAlias = type[_ScalarT] | dtype[_ScalarT] | _SupportsDType[dtype[_ScalarT]]

DTypeLike: TypeAlias = _DTypeLike[Any] | str | None
```

Now we can make sure that a function which accepts `DTypeLike | None` works as expected:

```py
import mini_numpy as np

def accepts_dtype(dtype: np.DTypeLike | None) -> None: ...

accepts_dtype(dtype=np.bool)
accepts_dtype(dtype=np.dtype[np.bool])
accepts_dtype(dtype=object)
accepts_dtype(dtype=np.object_)
accepts_dtype(dtype="U")
```

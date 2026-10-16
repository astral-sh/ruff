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

Now we can make sure that a function which accepts `DTypeLike | None` works as expected. A generic
function accepting `_DTypeLike[_ScalarT]` should also infer the scalar type from a scalar class. The
protocol union element describes instances with a `dtype` property, not the class object whose class
access exposes that property descriptor:

```py
from typing import TypeVar
import mini_numpy as np

def accepts_dtype(dtype: np.DTypeLike | None) -> None: ...

accepts_dtype(dtype=np.bool)
accepts_dtype(dtype=np.dtype[np.bool])  # error: [invalid-argument-type]
accepts_dtype(dtype=object)
accepts_dtype(dtype=np.object_)
accepts_dtype(dtype="U")

_ScalarT = TypeVar("_ScalarT", bound=np.generic)

def from_dtype_like(value: np._DTypeLike[_ScalarT]) -> np.dtype[_ScalarT]:
    raise NotImplementedError

reveal_type(from_dtype_like(np.bool))  # revealed: dtype[mini_numpy.bool[builtins.bool]]
```

## Scalar classes represented as aliases

NumPy's stubs represent some scalar classes as aliases to specialized generic classes. We recognize
these definitions as class objects and retain their specialization. Aliases to these names,
including renamed imports, refer to the same class objects.

`numpy/__init__.pyi`:

```pyi
from typing import TypeAlias

class _32Bit: ...
class _64Bit: ...
class _128Bit: ...

class signedinteger[T]:
    def __init__(self, value: int = 0) -> None: ...

class floating[T]: ...

int32 = signedinteger[_32Bit]
int64 = signedinteger[_64Bit]
int_ = int64
float32: TypeAlias = floating[_32Bit]
single: TypeAlias = float32

# A scalar declared as a class needs no compatibility handling.
class float64(floating[_64Bit]): ...

from numpy._typing._extended_precision import float128 as float128

# Other aliases are not scalar class definitions.
ScalarAlias = signedinteger[_64Bit]
int8 = list[int]

class Namespace:
    int64 = signedinteger[_64Bit]
```

NumPy declares extended-precision scalars in a Python module rather than a stub.

`numpy/_typing/__init__.py`:

```py
```

`numpy/_typing/_extended_precision.py`:

```py
import numpy as np

float128 = np.floating[np._128Bit]
```

The exception applies only to the known scalar definitions. Explicit subscriptions and other aliases
remain generic aliases, including aliases declared in unrelated stubs.

`other.pyi`:

```pyi
from numpy import signedinteger, _64Bit

int64 = signedinteger[_64Bit]
```

`main.py`:

```py
import numpy as np
import other
from numpy import int64 as Integer

def accepts_class(cls: type) -> None: ...
def construct[T](cls: type[T]) -> T:
    return cls()

accepts_class(np.int64)  # no diagnostic
accepts_class(Integer)  # no diagnostic
accepts_class(np.int_)  # no diagnostic
accepts_class(np.float64)  # no diagnostic
accepts_class(np.single)  # no diagnostic
accepts_class(np.float128)  # no diagnostic

reveal_type(construct(np.int32))  # revealed: signedinteger[_32Bit]
reveal_type(construct(Integer))  # revealed: signedinteger[_64Bit]
reveal_type(construct(np.float32))  # revealed: floating[_32Bit]
reveal_type(construct(np.float128))  # revealed: floating[_128Bit]

value: np.int64 = Integer(1)
scalar: type[np.int64] = Integer
wrong_scalar: type[np.int64] = np.int32  # error: [invalid-assignment]
isinstance(value, Integer)  # no diagnostic
issubclass(Integer, np.signedinteger)  # no diagnostic

accepts_class(np.signedinteger[np._64Bit])  # error: [invalid-argument-type]
accepts_class(np.ScalarAlias)  # error: [invalid-argument-type]
accepts_class(np.int8)  # error: [invalid-argument-type]
accepts_class(np.Namespace.int64)  # error: [invalid-argument-type]
accepts_class(other.int64)  # error: [invalid-argument-type]
```

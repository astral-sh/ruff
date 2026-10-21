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

## Scalar classes represented by aliases

Older NumPy stubs represent scalar classes as aliases for specialized generic bases. We retain these
alias types and their type arguments when they are imported or assigned to another name. We also
accept them as class arguments because the scalar names refer to classes at runtime.

`numpy/__init__.pyi`:

```pyi
from typing import Self, TypeAlias

class _32Bit: ...
class _64Bit: ...
class _128Bit: ...

class signedinteger[Precision]:
    def __new__(cls, value: int = ...) -> Self: ...

class unsignedinteger[Precision]: ...
class floating[Precision]: ...
class complexfloating[RealPrecision, ImagPrecision]: ...

int32: TypeAlias = signedinteger[_32Bit]
int64 = signedinteger[_64Bit]
int_ = int64
uint64: TypeAlias = unsignedinteger[_64Bit]
float32: TypeAlias = floating[_32Bit]
single: TypeAlias = float32
complex64: TypeAlias = complexfloating[_32Bit, _32Bit]

class float64(floating[_64Bit]): ...

from numpy._typing._extended_precision import float128 as float128

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

`main.py`:

```py
import numpy as np
from numpy import int64 as Integer

Alias = Integer

def accepts_int64(cls: type[np.int64]) -> None: ...
def accepts_class(cls: type) -> None: ...
def construct[T](cls: type[T]) -> T:
    return cls()

accepts_int64(Alias)  # no diagnostic
accepts_int64(np.int32)  # error: [invalid-argument-type]
accepts_class(np.int64)  # no diagnostic
accepts_class(Integer)  # no diagnostic
accepts_class(np.int_)  # no diagnostic
accepts_class(np.uint64)  # no diagnostic
accepts_class(np.float64)  # no diagnostic
accepts_class(np.single)  # no diagnostic
accepts_class(np.float128)  # no diagnostic
accepts_class(np.complex64)  # no diagnostic

reveal_type(construct(np.int32))  # revealed: signedinteger[_32Bit]
reveal_type(construct(Integer))  # revealed: signedinteger[_64Bit]
reveal_type(construct(np.float32))  # revealed: floating[_32Bit]
reveal_type(construct(np.float128))  # revealed: floating[_128Bit]

def construct_optional[T](cls: type[T] | None) -> T | None:
    return cls() if cls is not None else None

reveal_type(construct_optional(Integer))  # revealed: signedinteger[_64Bit] | None

def optional_class(flag: bool) -> None:
    candidate = Integer if flag else None
    reveal_type(construct_optional(candidate))  # revealed: signedinteger[_64Bit] | None

from types import GenericAlias

def identity[T](value: T) -> T:
    return value

retained_alias: GenericAlias = identity(Integer)  # no diagnostic
reveal_type(identity(Integer))  # revealed: <class 'signedinteger[_64Bit]'>

value: Alias = Integer(1)
reveal_type(value)  # revealed: signedinteger[_64Bit]

scalar_class: type[np.int64] = Integer  # no diagnostic
wrong_scalar: type[np.int64] = np.int32  # error: [invalid-assignment]
isinstance(value, Integer)  # no diagnostic
isinstance(value, Integer | str)  # no diagnostic
issubclass(Integer, np.signedinteger)  # no diagnostic

class Int64(Alias): ...  # no diagnostic

Scalar = np.int32 | np.int64
scalar: Scalar = Integer(1)  # no diagnostic
reveal_type(scalar)  # revealed: signedinteger[_64Bit]

def narrow(value: np.int32 | np.int64) -> None:
    if isinstance(value, Integer):
        reveal_type(value)  # revealed: signedinteger[_32Bit] | signedinteger[_64Bit]
    else:
        reveal_type(value)  # revealed: signedinteger[_32Bit] | signedinteger[_64Bit]
```

Scalar aliases can also register class-specific handlers:

`dispatch.py`:

```py
from functools import singledispatch
from numpy import int64 as Integer

@singledispatch
def process(value: object) -> None: ...
@process.register(Integer)  # no diagnostic
def process_integer(value: Integer) -> None: ...
```

The exception applies to aliases whose origins are NumPy's `signedinteger`, `unsignedinteger`,
`floating`, or `complexfloating` definitions. It also accepts explicit subscriptions and equivalent
aliases declared in other modules, even though explicit subscriptions produce aliases at runtime.
Other generic aliases and unrelated classes with the same names remain invalid class arguments.

`other.pyi`:

```pyi
from numpy import signedinteger as numpy_signedinteger, _64Bit

int64 = numpy_signedinteger[_64Bit]

class signedinteger[Precision]: ...
```

`compatibility.py`:

```py
import numpy as np
import other

def accepts_class(cls: type) -> None: ...

accepts_class(np.signedinteger[np._64Bit])  # no diagnostic
accepts_class(np.ScalarAlias)  # no diagnostic
accepts_class(np.Namespace.int64)  # no diagnostic
accepts_class(other.int64)  # no diagnostic

accepts_class(list[int])  # error: [invalid-argument-type]
accepts_class(np.int8)  # error: [invalid-argument-type]
accepts_class(other.signedinteger[np._64Bit])  # error: [invalid-argument-type]
```

## Scalar classes represented by class definitions

NumPy's newer stubs define scalar classes directly. These retain their concrete class identities and
allow precise narrowing.

`numpy/__init__.pyi`:

```pyi
from typing import Self, final

class _32Bit: ...
class _64Bit: ...

class signedinteger[Precision]:
    def __new__(cls, value: int = ...) -> Self: ...

@final
class int32(signedinteger[_32Bit]): ...

@final
class int64(signedinteger[_64Bit]): ...

longlong = int64
```

```py
import numpy as np
from numpy import int64 as Integer

Alias = Integer

def accepts_int64(cls: type[np.int64]) -> None: ...

accepts_int64(Alias)  # no diagnostic
accepts_int64(np.longlong)  # no diagnostic
accepts_int64(np.int32)  # error: [invalid-argument-type]

value: Alias = Integer(1)
reveal_type(value)  # revealed: int64

def narrow(value: np.int32 | np.int64) -> None:
    if isinstance(value, Integer):
        reveal_type(value)  # revealed: int64
    else:
        reveal_type(value)  # revealed: int32
```

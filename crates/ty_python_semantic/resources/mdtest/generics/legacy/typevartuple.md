# Legacy `TypeVarTuple`

```toml
[environment]
python-version = "3.11"
```

## Definition

### Valid

```py
from typing import TypeVarTuple

Ts = TypeVarTuple("Ts")
reveal_type(type(Ts))  # revealed: <class 'TypeVarTuple'>
reveal_type(Ts)  # revealed: TypeVarTuple
reveal_type(Ts.__name__)  # revealed: Literal["Ts"]
```

The `TypeVarTuple` name can also be provided as a keyword argument:

```py
from typing import TypeVarTuple

Ts = TypeVarTuple(name="Ts")
reveal_type(Ts.__name__)  # revealed: Literal["Ts"]
```

### Must be directly assigned to a variable

```py
from typing import TypeVarTuple

Ts = TypeVarTuple("Ts")

# error: [invalid-legacy-type-variable]
Ts1: TypeVarTuple = TypeVarTuple("Ts1")

# error: [invalid-legacy-type-variable]
tuple_with_typevartuple = ("foo", TypeVarTuple("Us"))
reveal_type(tuple_with_typevartuple[1])  # revealed: TypeVarTuple
```

### `TypeVarTuple` parameter must match variable name

```py
from typing import Generic, TypeVarTuple

Ts1 = TypeVarTuple("Ts1")

# error: [mismatched-type-name]
Ts2 = TypeVarTuple("Ts3")

class Array(Generic[*Ts2]): ...
```

### Bounds and constraints

`TypeVarTuple` does not allow defining bounds or constraints.

```py
from typing import TypeVarTuple

# error: [invalid-legacy-type-variable]
Ts1 = TypeVarTuple("Ts1", bound=int)
# error: [invalid-legacy-type-variable]
Ts2 = TypeVarTuple("Ts2", int, str)
```

### Variance

Legacy `TypeVarTuple` accepts `covariant` and `contravariant` arguments. A `TypeVarTuple` with no
variance specified is invariant, and a `TypeVarTuple` with `infer_variance=True` uses variance
inference.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Generic, TypeVarTuple

Ts = TypeVarTuple("Ts")

class InvariantArray(Generic[*Ts]):
    values: tuple[*Ts]

invariant_out: InvariantArray[object] = InvariantArray[int]()  # error: [invalid-assignment]
invariant_in: InvariantArray[int] = InvariantArray[object]()  # error: [invalid-assignment]

OutTs = TypeVarTuple("OutTs", covariant=True)

class CovariantArray(Generic[*OutTs]):
    def get(self) -> tuple[*OutTs]:
        raise NotImplementedError

covariant_ok: CovariantArray[object] = CovariantArray[int]()
covariant_error: CovariantArray[int] = CovariantArray[object]()  # error: [invalid-assignment]

InTs = TypeVarTuple("InTs", contravariant=True)

class ContravariantArray(Generic[*InTs]):
    def set(self, value: tuple[*InTs]) -> None:
        raise NotImplementedError

contravariant_ok: ContravariantArray[int] = ContravariantArray[object]()
contravariant_error: ContravariantArray[object] = ContravariantArray[int]()  # error: [invalid-assignment]

InferredOutTs = TypeVarTuple("InferredOutTs", infer_variance=True)

class InferredCovariantArray(Generic[*InferredOutTs]):
    def get(self) -> tuple[*InferredOutTs]:
        raise NotImplementedError

inferred_covariant_ok: InferredCovariantArray[object] = InferredCovariantArray[int]()
inferred_covariant_error: InferredCovariantArray[int] = InferredCovariantArray[object]()  # error: [invalid-assignment]

InferredInTs = TypeVarTuple("InferredInTs", infer_variance=True)

class InferredContravariantArray(Generic[*InferredInTs]):
    def set(self, value: tuple[*InferredInTs]) -> None:
        raise NotImplementedError

inferred_contravariant_ok: InferredContravariantArray[int] = InferredContravariantArray[object]()
# error: [invalid-assignment]
inferred_contravariant_error: InferredContravariantArray[object] = InferredContravariantArray[int]()
```

The variance arguments must have statically known boolean values, and `infer_variance=True` cannot
be combined with an explicit variance.

```py
from typing import TypeVarTuple

def cond() -> bool:
    return True

# error: [invalid-legacy-type-variable]
Both = TypeVarTuple("Both", covariant=True, contravariant=True)
# error: [invalid-legacy-type-variable]
AmbiguousCovariant = TypeVarTuple("AmbiguousCovariant", covariant=cond())
# error: [invalid-legacy-type-variable]
AmbiguousContravariant = TypeVarTuple("AmbiguousContravariant", contravariant=cond())
# error: [invalid-legacy-type-variable]
AmbiguousInferVariance = TypeVarTuple("AmbiguousInferVariance", infer_variance=cond())
# error: [invalid-legacy-type-variable]
CovariantAndInferred = TypeVarTuple("CovariantAndInferred", covariant=True, infer_variance=True)
```

### Must always be unpacked

```py
from typing import TypeVarTuple

Ts = TypeVarTuple("Ts")

# error: [invalid-type-form]
def invalid(x: Ts) -> None: ...

# error: [invalid-type-form]
def invalid_args(*args: Ts) -> None: ...
def valid(x: tuple[*Ts]) -> tuple[*Ts]:
    return x
```

## Generic Classes

### Explicit specialization

```py
from typing import Generic, TypeVar, TypeVarTuple

T = TypeVar("T")
Ts = TypeVarTuple("Ts")
U = TypeVar("U")

class Array(Generic[*Ts]):
    def shape(self) -> tuple[*Ts]:
        raise NotImplementedError

class Sandwich(Generic[T, *Ts, U]):
    def parts(self) -> tuple[T, *Ts, U]:
        raise NotImplementedError

def f(
    array: Array[int, str],
    empty: Array[()],
    sandwich1: Sandwich[int, bool, str],
    sandwich2: Sandwich[int, str],
) -> None:
    reveal_type(array)  # revealed: Array[int, str]
    reveal_type(array.shape())  # revealed: tuple[int, str]
    reveal_type(empty)  # revealed: Array[()]
    reveal_type(empty.shape())  # revealed: tuple[()]
    reveal_type(sandwich1.parts())  # revealed: tuple[int, bool, str]
    reveal_type(sandwich2.parts())  # revealed: tuple[int, str]
```

### `typing.Unpack` spelling

```py
from typing import Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

class Array(Generic[Unpack[Ts]]): ...

def f(array: Array[int, str], empty: Array[()]) -> None:
    reveal_type(array)  # revealed: Array[int, str]
    reveal_type(empty)  # revealed: Array[()]
```

### Assignment checks

```py
from typing import Generic, TypeVarTuple

Ts = TypeVarTuple("Ts")

class Array(Generic[*Ts]): ...

def takes_int_str(x: Array[int, str]) -> None: ...
def takes_int_str_tuple(x: tuple[int, str]) -> None: ...
def f(x: Array[int, str], y: Array[str, int], xs: tuple[int, str], ys: tuple[str, int]) -> None:
    takes_int_str(x)
    takes_int_str(y)  # error: [invalid-argument-type]
    takes_int_str_tuple(xs)
    takes_int_str_tuple(ys)  # error: [invalid-argument-type]
```

### Multiple Type Variable Tuples not allowed

Only one type variable tuple can appear in a type parameter list.

```toml
[environment]
python-version = "3.12"
```

```py
# error: [invalid-type-form]
class Array[*Ts1, *Ts2]: ...
```

## Functions

### Tuple arguments and returns

```py
from typing import TypeVar, TypeVarTuple

T = TypeVar("T")
Ts = TypeVarTuple("Ts")
U = TypeVar("U")

def echo(x: tuple[*Ts]) -> tuple[*Ts]:
    return x

def with_prefix(x: T, y: tuple[*Ts]) -> tuple[T, *Ts]:
    raise NotImplementedError

def with_suffix(x: tuple[*Ts], y: U) -> tuple[*Ts, U]:
    raise NotImplementedError

def with_both(x: tuple[T, *Ts, U]) -> tuple[*Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(echo((i, s)))  # revealed: tuple[int, str]
    reveal_type(echo(()))  # revealed: tuple[()]
    reveal_type(with_prefix(i, (s, b)))  # revealed: tuple[int, str, bool]
    reveal_type(with_suffix((i, s), b))  # revealed: tuple[int, str, bool]
    reveal_type(with_both((i, s, b)))  # revealed: tuple[str]
```

### Starred variadic parameters

When a `TypeVarTuple` appears in `*args`, the argument tuple keeps the exact element types from the
call site.

```py
from typing import TypeVar, TypeVarTuple

T = TypeVar("T")
Ts = TypeVarTuple("Ts")

def args_to_tuple(*args: *Ts) -> tuple[*Ts]:
    reveal_type(args)  # revealed: tuple[*Ts@args_to_tuple]
    raise NotImplementedError

def first_and_rest(first: T, *rest: *Ts) -> tuple[T, *Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(args_to_tuple(i, s))  # revealed: tuple[int, str]
    reveal_type(args_to_tuple())  # revealed: tuple[()]
    reveal_type(first_and_rest(i, s, b))  # revealed: tuple[int, str, bool]
```

### `Unpack` spelling for variadic parameters

The legacy `Unpack` spelling is equivalent to `*Ts` in a variadic parameter annotation.

```py
from typing import Callable, TypeVar, TypeVarTuple, Unpack

T = TypeVar("T")
Ts = TypeVarTuple("Ts")

def call_with_args(func: Callable[[Unpack[Ts]], T], *args: Unpack[Ts]) -> T:
    raise NotImplementedError

def takes_int_str(i: int, s: str) -> bool:
    return True

def f(i: int, s: str) -> None:
    reveal_type(call_with_args(takes_int_str, i, s))  # revealed: bool
```

### Callable parameters

`Callable` accepts unpacked `TypeVarTuple`s in its positional parameter list.

```py
from typing import Callable, TypeVarTuple

Ts = TypeVarTuple("Ts")

def f(callback: Callable[[int, *Ts], tuple[*Ts]]) -> None:
    reveal_type(callback)  # revealed: (int, /, *Ts@f) -> tuple[*Ts@f]
```

### Length-sensitive inference

```py
from typing import TypeVarTuple

Ts = TypeVarTuple("Ts")

def same_shape(x: tuple[*Ts], y: tuple[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(same_shape((i, s), (b, i)))  # revealed: tuple[int, str | int]
    same_shape((i,), (s, b))  # error: [invalid-argument-type]
```

## Type concatenation

A type variable tuple can be combined with fixed leading or trailing types.

```py
from typing import Generic, TypeVar, TypeVarTuple

T = TypeVar("T")
Shape = TypeVarTuple("Shape")

class Array(Generic[*Shape]): ...
class Batch: ...
class Channels: ...
class Height: ...
class Width: ...

def add_batch_axis(x: Array[*Shape]) -> Array[Batch, *Shape]:
    raise NotImplementedError

def del_batch_axis(x: Array[Batch, *Shape]) -> Array[*Shape]:
    raise NotImplementedError

def add_batch_channels(x: Array[*Shape]) -> Array[Batch, *Shape, Channels]:
    raise NotImplementedError

def del_channels_axis(x: Array[*Shape, Channels]) -> Array[*Shape]:
    raise NotImplementedError

def prefix_tuple(x: T, y: tuple[*Shape]) -> tuple[T, *Shape]:
    raise NotImplementedError

def f(a: Array[Height, Width], c: Array[Height, Width, Channels]) -> None:
    b = add_batch_axis(a)
    reveal_type(b)  # revealed: Array[Batch, Height, Width]
    reveal_type(del_batch_axis(b))  # revealed: Array[Height, Width]
    reveal_type(add_batch_channels(a))  # revealed: Array[Batch, Height, Width, Channels]
    reveal_type(del_channels_axis(c))  # revealed: Array[Height, Width]
    reveal_type(prefix_tuple(1, (True, "a")))  # revealed: tuple[Literal[1], Literal[True], Literal["a"]]
```

## Type Aliases

### Legacy generic aliases

```py
from typing import TypeVar, TypeVarTuple

T = TypeVar("T")
Ts = TypeVarTuple("Ts")
U = TypeVar("U")

Alias = tuple[T, *Ts, U]
Prefix = tuple[int, *Ts]
Suffix = tuple[*Ts, str]

def f(
    alias: Alias[int, bool, str],
    short_alias: Alias[int, str],
    prefix: Prefix[bool, str],
    suffix: Suffix[int, bool],
) -> None:
    reveal_type(alias)  # revealed: tuple[int, bool, str]
    reveal_type(short_alias)  # revealed: tuple[int, str]
    reveal_type(prefix)  # revealed: tuple[int, bool, str]
    reveal_type(suffix)  # revealed: tuple[int, bool, str]
```

### Unpacked tuple type arguments

```py
from typing import TypeVarTuple

Ts = TypeVarTuple("Ts")

Alias = tuple[int, *Ts]

def f(x: Alias[*tuple[str, bool]], y: Alias[*tuple[str, ...]]) -> None:
    reveal_type(x)  # revealed: tuple[int, str, bool]
    reveal_type(y)  # revealed: tuple[int, *tuple[str, ...]]
```

## Invalid Tuple Forms

### Only one variadic unpack

```py
from typing import TypeVarTuple

Ts = TypeVarTuple("Ts")

def f(
    ok1: tuple[int, *Ts],
    ok2: tuple[int, *Ts, str],
    bad1: tuple[*Ts, *tuple[str, ...]],  # error: [invalid-type-form]
    bad2: tuple[*tuple[str, ...], *Ts],  # error: [invalid-type-form]
) -> None: ...
```

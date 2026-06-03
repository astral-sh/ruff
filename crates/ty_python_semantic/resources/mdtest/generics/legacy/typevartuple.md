# Legacy `TypeVarTuple`

```toml
[environment]
python-version = "3.11"
```

The tests in this file focus on how `TypeVarTuple`s are defined and specialized using the legacy
notation. Shared uses of `TypeVarTuple`s are tested with PEP 695 syntax in
`../pep695/typevartuple.md`; alternate `Unpack` spelling is tested in `unpack.md`.

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

Ts_Co = TypeVarTuple("Ts_Co", covariant=True)

class CovariantArray(Generic[*Ts_Co]):
    def get(self) -> tuple[*Ts_Co]:
        raise NotImplementedError

covariant_ok: CovariantArray[object] = CovariantArray[int]()
covariant_error: CovariantArray[int] = CovariantArray[object]()  # error: [invalid-assignment]

Ts_Contra = TypeVarTuple("Ts_Contra", contravariant=True)

class ContravariantArray(Generic[*Ts_Contra]):
    def set(self, value: tuple[*Ts_Contra]) -> None:
        raise NotImplementedError

contravariant_ok: ContravariantArray[int] = ContravariantArray[object]()
contravariant_error: ContravariantArray[object] = ContravariantArray[int]()  # error: [invalid-assignment]

Ts_Inferred_Co = TypeVarTuple("Ts_Inferred_Co", infer_variance=True)

class InferredCovariantArray(Generic[*Ts_Inferred_Co]):
    def get(self) -> tuple[*Ts_Inferred_Co]:
        raise NotImplementedError

inferred_covariant_ok: InferredCovariantArray[object] = InferredCovariantArray[int]()
inferred_covariant_error: InferredCovariantArray[int] = InferredCovariantArray[object]()  # error: [invalid-assignment]

Ts_Inferred_Contra = TypeVarTuple("Ts_Inferred_Contra", infer_variance=True)

class InferredContravariantArray(Generic[*Ts_Inferred_Contra]):
    def set(self, value: tuple[*Ts_Inferred_Contra]) -> None:
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

### Inferred specialization from construction

Calling a generic class without explicit type arguments infers its specialization from the
constructor arguments.

```py
from typing import Generic, TypeVarTuple

Ts = TypeVarTuple("Ts")

class Array(Generic[*Ts]):
    def __init__(self, shape: tuple[*Ts]) -> None:
        self.shape = shape

def f(i: int, s: str) -> None:
    reveal_type(Array((i, s)))  # revealed: Array[int, str]
    reveal_type(Array(()))  # revealed: Array[()]
```

### Default type arguments

A defaulted type variable tuple supplies its unpacked tuple when the generic class is not explicitly
specialized. Explicit type arguments override the default.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts", default=Unpack[tuple[int, str]])

class Array(Generic[*Ts]):
    def shape(self) -> tuple[*Ts]:
        raise NotImplementedError

def f(default: Array, explicit: Array[bool]) -> None:
    reveal_type(default)  # revealed: Array[int, str]
    reveal_type(default.shape())  # revealed: tuple[int, str]
    reveal_type(explicit)  # revealed: Array[bool]
    reveal_type(explicit.shape())  # revealed: tuple[bool]
    reveal_type(Array())  # revealed: Array[int, str]
    reveal_type(Array[bytes, bool]())  # revealed: Array[bytes, bool]
```

### Unspecified type arguments

An unsubscripted variadic generic uses an unknown-length tuple of `Any` arguments, allowing typed
and dynamic uses of the class to interoperate.

```py
from typing import Any, Generic, TypeVarTuple

Ts = TypeVarTuple("Ts")

class Shelf(Generic[*Ts]):
    def contents(self) -> tuple[*Ts]:
        raise NotImplementedError

def needs_two_items(x: Shelf[int, str]) -> None:
    raise NotImplementedError

def needs_dynamic_items(x: Shelf) -> None:
    raise NotImplementedError

def f(
    dynamic: Shelf,
    explicitly_dynamic: Shelf[*tuple[Any, ...]],
    fixed: Shelf[int, str],
) -> None:
    reveal_type(dynamic)  # revealed: Shelf[*tuple[Any, ...]]
    reveal_type(dynamic.contents())  # revealed: tuple[Any, ...]
    reveal_type(explicitly_dynamic)  # revealed: Shelf[*tuple[Any, ...]]
    reveal_type(explicitly_dynamic.contents())  # revealed: tuple[Any, ...]
    needs_two_items(dynamic)
    needs_dynamic_items(fixed)
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
    long_alias: Alias[int, bool, bytes, str],
    short_alias: Alias[int, str],
    prefix: Prefix[bool, str],
    suffix: Suffix[int, bool],
) -> None:
    reveal_type(alias)  # revealed: tuple[int, bool, str]
    reveal_type(long_alias)  # revealed: tuple[int, bool, bytes, str]
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

### Unspecified alias type arguments

A bare variadic alias substitutes an unknown-length tuple of `Any`.

```py
from typing import Any, TypeVarTuple

Ts = TypeVarTuple("Ts")

Headered = tuple[bytes, *Ts]

def f(raw: Headered, explicit: Headered[*tuple[Any, ...]]) -> None:
    reveal_type(raw)  # revealed: tuple[bytes, *tuple[Any, ...]]
    reveal_type(explicit)  # revealed: tuple[bytes, *tuple[Any, ...]]
```

### Variadic substitutions

Legacy aliases can forward a type variable tuple and split an unbounded tuple to satisfy a fixed
trailing type argument.

```py
from typing import TypeVar, TypeVarTuple

Start = TypeVar("Start")
End = TypeVar("End")
Ts = TypeVarTuple("Ts")

Payload = tuple[bytes, *Ts]
CountedPayload = Payload[int, *Ts]
Started = tuple[Start, *Ts]
Terminated = tuple[*Ts, End]

def f(
    forwarded: CountedPayload[str, bool],
    leading_split: Started[*tuple[str, ...]],
    split: Terminated[*tuple[str, ...]],
    retained: Terminated[*tuple[str, ...], bytes],
    combined: Terminated[int, *tuple[str, ...]],
) -> None:
    reveal_type(forwarded)  # revealed: tuple[bytes, int, str, bool]
    reveal_type(leading_split)  # revealed: tuple[str, *tuple[str, ...]]
    reveal_type(split)  # revealed: tuple[*tuple[str, ...], str]
    reveal_type(retained)  # revealed: tuple[*tuple[str, ...], bytes]
    reveal_type(combined)  # revealed: tuple[int, *tuple[str, ...], str]
```

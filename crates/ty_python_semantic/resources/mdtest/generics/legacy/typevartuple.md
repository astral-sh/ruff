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
# TODO: revealed: Literal["Ts"]
reveal_type(Ts.__name__)  # revealed: str
```

The `TypeVarTuple` name can also be provided as a keyword argument:

```py
from typing import TypeVarTuple

Ts = TypeVarTuple(name="Ts")
# TODO: revealed: Literal["Ts"]
reveal_type(Ts.__name__)  # revealed: str
```

### Must be directly assigned to a variable

```py
from typing import TypeVarTuple

Ts = TypeVarTuple("Ts")

# TODO: error: [invalid-legacy-type-variable]
Ts1: TypeVarTuple = TypeVarTuple("Ts1")

# TODO: error: [invalid-legacy-type-variable]
tuple_with_typevartuple = ("foo", TypeVarTuple("Us"))
reveal_type(tuple_with_typevartuple[1])  # revealed: TypeVarTuple
```

### `TypeVarTuple` parameter must match variable name

```py
from typing import Generic, TypeVarTuple

Ts1 = TypeVarTuple("Ts1")

# TODO: error: [mismatched-type-name]
Ts2 = TypeVarTuple("Ts3")

class Array(Generic[*Ts2]): ...
```

### Bounds and constraints

`TypeVarTuple` does not allow defining bounds or constraints.

```py
from typing import TypeVarTuple

# TODO: error: [invalid-legacy-type-variable]
# error: [unknown-argument]
Ts1 = TypeVarTuple("Ts1", bound=int)
# TODO: error: [invalid-legacy-type-variable]
# error: [too-many-positional-arguments]
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

invariant_out: InvariantArray[object] = InvariantArray[int]()  # TODO: error: [invalid-assignment]
invariant_in: InvariantArray[int] = InvariantArray[object]()  # TODO: error: [invalid-assignment]

# error: [unknown-argument]
Ts_Co = TypeVarTuple("Ts_Co", covariant=True)

class CovariantArray(Generic[*Ts_Co]):
    def get(self) -> tuple[*Ts_Co]:
        raise NotImplementedError

covariant_ok: CovariantArray[object] = CovariantArray[int]()
covariant_error: CovariantArray[int] = CovariantArray[object]()  # TODO: error: [invalid-assignment]

# error: [unknown-argument]
Ts_Contra = TypeVarTuple("Ts_Contra", contravariant=True)

class ContravariantArray(Generic[*Ts_Contra]):
    def set(self, value: tuple[*Ts_Contra]) -> None:
        raise NotImplementedError

contravariant_ok: ContravariantArray[int] = ContravariantArray[object]()
contravariant_error: ContravariantArray[object] = ContravariantArray[int]()  # TODO: error: [invalid-assignment]

# error: [unknown-argument]
Ts_Inferred_Co = TypeVarTuple("Ts_Inferred_Co", infer_variance=True)

class InferredCovariantArray(Generic[*Ts_Inferred_Co]):
    def get(self) -> tuple[*Ts_Inferred_Co]:
        raise NotImplementedError

inferred_covariant_ok: InferredCovariantArray[object] = InferredCovariantArray[int]()
inferred_covariant_error: InferredCovariantArray[int] = InferredCovariantArray[object]()  # TODO: error: [invalid-assignment]

# error: [unknown-argument]
Ts_Inferred_Contra = TypeVarTuple("Ts_Inferred_Contra", infer_variance=True)

class InferredContravariantArray(Generic[*Ts_Inferred_Contra]):
    def set(self, value: tuple[*Ts_Inferred_Contra]) -> None:
        raise NotImplementedError

inferred_contravariant_ok: InferredContravariantArray[int] = InferredContravariantArray[object]()
# TODO: error: [invalid-assignment]
inferred_contravariant_error: InferredContravariantArray[object] = InferredContravariantArray[int]()
```

The variance arguments must have statically known boolean values, and `infer_variance=True` cannot
be combined with an explicit variance.

```py
from typing import TypeVarTuple

def cond() -> bool:
    return True

# TODO: error: [invalid-legacy-type-variable]
# error: [unknown-argument]
# error: [unknown-argument]
Both = TypeVarTuple("Both", covariant=True, contravariant=True)
# TODO: error: [invalid-legacy-type-variable]
# error: [unknown-argument]
AmbiguousCovariant = TypeVarTuple("AmbiguousCovariant", covariant=cond())
# TODO: error: [invalid-legacy-type-variable]
# error: [unknown-argument]
AmbiguousContravariant = TypeVarTuple("AmbiguousContravariant", contravariant=cond())
# TODO: error: [invalid-legacy-type-variable]
# error: [unknown-argument]
AmbiguousInferVariance = TypeVarTuple("AmbiguousInferVariance", infer_variance=cond())
# TODO: error: [invalid-legacy-type-variable]
# error: [unknown-argument]
# error: [unknown-argument]
CovariantAndInferred = TypeVarTuple("CovariantAndInferred", covariant=True, infer_variance=True)
```

## Generic Classes

### Explicit specialization

```py
from typing import Generic, TypeVar, TypeVarTuple

T = TypeVar("T")
Ts = TypeVarTuple("Ts")
U = TypeVar("U")

class Simple(Generic[*Ts]):
    attr: tuple[*Ts]

# TODO: revealed: tuple[()]
reveal_type(Simple[()]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, str]
reveal_type(Simple[int, str]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, str]
reveal_type(Simple[*tuple[int, str]]().attr)  # revealed: Unknown

# TODO: error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
# TODO: revealed: tuple[Unknown]
reveal_type(Simple[[int, str]]().attr)  # revealed: Unknown
# TODO: error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
# TODO: revealed: tuple[Unknown, ...]
reveal_type(Simple[*[int, str]]().attr)  # revealed: Unknown
```

```py
class Prefix(Generic[T, *Ts]):
    # error: [unbound-type-variable]
    attr: tuple[T, *Ts]

# TODO: revealed: tuple[int]
reveal_type(Prefix[int]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, bool]
reveal_type(Prefix[int, bool]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, bool, str]
reveal_type(Prefix[int, bool, str]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, bool, str]
reveal_type(Prefix[int, *tuple[bool, str]]().attr)  # revealed: Unknown

# TODO: Should this raise an error?
# TODO: revealed: tuple[Unknown, *tuple[Unknown, ...]]
reveal_type(Prefix().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

```py
class Suffix(Generic[*Ts, T]):
    # error: [unbound-type-variable]
    attr: tuple[*Ts, T]

# TODO: revealed: tuple[int]
reveal_type(Suffix[int]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, str]
reveal_type(Suffix[int, str]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, str, bool]
reveal_type(Suffix[int, str, bool]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, str, bool]
reveal_type(Suffix[*tuple[int, str], bool]().attr)  # revealed: Unknown

# TODO: Should this raise an error?
# TODO: revealed: tuple[*tuple[Unknown, ...], Unknown]
reveal_type(Suffix().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

```py
class Between(Generic[T, *Ts, U]):
    # error: [unbound-type-variable]
    # error: [unbound-type-variable]
    attr: tuple[T, *Ts, U]

# TODO: revealed: tuple[int, str]
reveal_type(Between[int, str]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, bool, str]
reveal_type(Between[int, bool, str]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, bool, bytes, str]
reveal_type(Between[int, bool, bytes, str]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, bool, str]
reveal_type(Between[int, *tuple[bool], str]().attr)  # revealed: Unknown

# TODO: revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
reveal_type(Between().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: error: [invalid-type-arguments] "No type argument provided for required type variable `U` of class `Between`"
# TODO: revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
reveal_type(Between[int]().attr)  # revealed: Unknown
```

### Inferred specialization from construction

Calling a generic class without explicit type arguments infers its specialization from the
constructor arguments.

```py
from typing import Generic, TypeVarTuple

Ts = TypeVarTuple("Ts")

class Positional(Generic[*Ts]):
    def __init__(self, shape: tuple[*Ts]) -> None:
        self.shape = shape

class Variadic(Generic[*Ts]):
    def __init__(self, *shape: *Ts) -> None:
        self.shape = shape

# TODO: revealed: Positional[()]
reveal_type(Positional(()))  # revealed: Positional
# TODO: revealed: Positional[int, str]
reveal_type(Positional((1, "a")))  # revealed: Positional

# TODO: revealed: Variadic[()]
reveal_type(Variadic())  # revealed: Variadic
# TODO: revealed: Variadic[int, str]
reveal_type(Variadic(1, "a"))  # revealed: Variadic

def _(i: int, s: str) -> None:
    # TODO: revealed: Positional[int, str]
    reveal_type(Positional((i, s)))  # revealed: Positional
    # TODO: revealed: Variadic[int, str]
    reveal_type(Variadic(i, s))  # revealed: Variadic
```

### Unspecified type arguments

When a generic class parameterized by a type variable tuple is used without any type parameters and
the `TypeVarTuple` has no default value, it behaves as if the type variable tuple was substituted
with `tuple[Any, ...]`. ty represents the missing type information as `tuple[Unknown, ...]`,
distinguishing it from an explicitly provided `tuple[Any, ...]`.

```py
from typing import Generic, TypeVarTuple

Ts = TypeVarTuple("Ts")

class Unspecified(Generic[*Ts]):
    attr: tuple[*Ts]

unspecified = Unspecified()
# TODO: revealed: Unspecified[*tuple[Unknown, ...]]
reveal_type(unspecified)  # revealed: Unspecified
# TODO: revealed: tuple[Unknown, ...]
reveal_type(unspecified.attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
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

class WithDefault(Generic[*Ts]):
    attr: tuple[*Ts]

# TODO: revealed: tuple[int, str]
reveal_type(WithDefault().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[bool, bytes]
reveal_type(WithDefault[bool, bytes]().attr)  # revealed: Unknown
```

### Backported default type arguments

`typing_extensions.TypeVarTuple` backports the `default` parameter to older Python versions.

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Generic
from typing_extensions import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts", default=Unpack[tuple[int, str]])

class WithBackportedDefault(Generic[*Ts]):
    attr: tuple[*Ts]

# TODO: revealed: tuple[int, str]
reveal_type(WithBackportedDefault().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

## Type Aliases

### Legacy generic aliases

```py
from typing import TypeVar, TypeVarTuple

T = TypeVar("T")
Ts = TypeVarTuple("Ts")
U = TypeVar("U")

Simple = tuple[*Ts]
Between = tuple[T, *Ts, U]
Prefix = tuple[T, *Ts]
Suffix = tuple[*Ts, U]

def _(
    a1: Simple[()],
    a2: Simple[int, str],
    a3: Between[int, str],
    a4: Between[int, bool, str],
    a5: Between[int, bool, bytes, str],
    a6: Prefix[bool],
    a7: Prefix[bool, int, str],
    a8: Suffix[bool],
    a9: Suffix[int, str, bool],
    # TODO: error: [invalid-type-arguments] "No type argument provided for required type variable `U`"
    a10: Between[int],
):
    # TODO: revealed: tuple[()]
    reveal_type(a1)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str]
    reveal_type(a2)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str]
    reveal_type(a3)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, bool, str]
    reveal_type(a4)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, bool, bytes, str]
    reveal_type(a5)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[bool]
    reveal_type(a6)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[bool, int, str]
    reveal_type(a7)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[bool]
    reveal_type(a8)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(a9)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
    reveal_type(a10)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

### Unpacked tuple type arguments

```py
from typing import TypeVarTuple

Ts = TypeVarTuple("Ts")

Alias = tuple[int, *Ts]

def _(a1: Alias[*tuple[str, bool]], a2: Alias[*tuple[str, ...]]) -> None:
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(a1)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, *tuple[str, ...]]
    reveal_type(a2)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

### Unspecified alias type arguments

A bare variadic alias substitutes an unknown-length tuple of `Any`.

```py
from typing import Any, TypeVarTuple

Ts = TypeVarTuple("Ts")

Alias = tuple[bytes, *Ts]

def _(a1: Alias, a2: Alias[*tuple[Any, ...]]) -> None:
    # TODO: revealed: tuple[bytes, *tuple[Unknown, ...]]
    reveal_type(a1)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[bytes, *tuple[Any, ...]]
    reveal_type(a2)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

### Splitting arbitrary-length tuples

```py
from typing import TypeVar, TypeVarTuple

T = TypeVar("T")
Ts = TypeVarTuple("Ts")

First = tuple[*Ts, T]
Second = tuple[T, *Ts]

def _(
    f1: First[*tuple[int, ...]],
    f2: First[*tuple[int, ...], str],
    s1: Second[*tuple[int, ...]],
    s2: Second[str, *tuple[int, ...]],
):
    # TODO: revealed: tuple[*tuple[int, ...], int]
    reveal_type(f1)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[*tuple[int, ...], str]
    reveal_type(f2)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, *tuple[int, ...]]
    reveal_type(s1)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[str, *tuple[int, ...]]
    reveal_type(s2)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

### Variadic substitutions

Legacy aliases can forward a type variable tuple.

```py
from typing import TypeVar, TypeVarTuple

Ts = TypeVarTuple("Ts")

First = tuple[bytes, *Ts]
Second = First[int, *Ts]

def f(a1: First[str, bool], a2: Second[str, bool]) -> None:
    # TODO: revealed: tuple[bytes, str, bool]
    reveal_type(a1)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[bytes, int, str, bool]
    reveal_type(a2)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

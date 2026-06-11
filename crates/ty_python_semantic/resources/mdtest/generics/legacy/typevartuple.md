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

The `bound` parameter was added to `typing.TypeVarTuple` in Python 3.15. On older Python versions,
using it is invalid. Constraints are not supported in any Python version.

#### Before Python 3.15

```toml
[environment]
python-version = "3.14"
```

```py
from typing import TypeVarTuple

# error: [invalid-legacy-type-variable] "The `bound` parameter of `typing.TypeVarTuple` was added in Python 3.15"
Ts1 = TypeVarTuple("Ts1", bound=int)
# error: [invalid-legacy-type-variable]
Ts2 = TypeVarTuple("Ts2", int, str)
```

#### Python 3.15

ty does not yet support the `bound` parameter when targeting Python 3.15.

```toml
[environment]
python-version = "3.15"
```

```py
from typing import TypeVarTuple

# error: [invalid-legacy-type-variable] "The `bound` argument for `TypeVarTuple` is not supported"
Ts = TypeVarTuple("Ts", bound=int)
```

#### `typing_extensions.TypeVarTuple`

`typing_extensions.TypeVarTuple` exposes the `bound` parameter on older Python versions. ty
recognizes the backport but does not yet support the parameter.

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import TypeVarTuple

# error: [invalid-legacy-type-variable] "The `bound` argument for `TypeVarTuple` is not supported"
Ts = TypeVarTuple("Ts", bound=int)
```

### Variance

Legacy `TypeVarTuple` accepts `covariant` and `contravariant` arguments. A `TypeVarTuple` with no
variance specified is invariant, and a `TypeVarTuple` with `infer_variance=True` uses variance
inference. These parameters were added to `typing.TypeVarTuple` in Python 3.15.

#### Before Python 3.15

```toml
[environment]
python-version = "3.14"
```

```py
from typing import TypeVarTuple

# error: [invalid-legacy-type-variable] "The `covariant` parameter of `typing.TypeVarTuple` was added in Python 3.15"
Ts_Co = TypeVarTuple("Ts_Co", covariant=True)
# error: [invalid-legacy-type-variable] "The `contravariant` parameter of `typing.TypeVarTuple` was added in Python 3.15"
Ts_Contra = TypeVarTuple("Ts_Contra", contravariant=True)
# error: [invalid-legacy-type-variable] "The `infer_variance` parameter of `typing.TypeVarTuple` was added in Python 3.15"
Ts_Inferred = TypeVarTuple("Ts_Inferred", infer_variance=True)
```

#### Python 3.15

```toml
[environment]
python-version = "3.15"
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

#### `typing_extensions.TypeVarTuple`

`typing_extensions.TypeVarTuple` backports the variance parameters to older Python versions.

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import TypeVarTuple

Ts_Co = TypeVarTuple("Ts_Co", covariant=True)
Ts_Contra = TypeVarTuple("Ts_Contra", contravariant=True)
Ts_Inferred = TypeVarTuple("Ts_Inferred", infer_variance=True)
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
reveal_type(Simple[()]().attr)  # revealed: tuple[tuple[()], ...]
# TODO: revealed: tuple[int, str]
reveal_type(Simple[int, str]().attr)  # revealed: tuple[tuple[int, str], ...]
# TODO: revealed: tuple[int, str]
reveal_type(Simple[*tuple[int, str]]().attr)  # revealed: tuple[tuple[int, str], ...]

# TODO: revealed: tuple[Unknown]
# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
reveal_type(Simple[[int, str]]().attr)  # revealed: tuple[tuple[Unknown], ...]
# TODO: revealed: tuple[Unknown, ...]
# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
reveal_type(Simple[*[int, str]]().attr)  # revealed: tuple[tuple[Unknown, ...], ...]
```

```py
class Prefix(Generic[T, *Ts]):
    attr: tuple[T, *Ts]

# TODO: revealed: tuple[int]
reveal_type(Prefix[int]().attr)  # revealed: tuple[int, *tuple[tuple[()], ...]]
# TODO: revealed: tuple[int, bool]
reveal_type(Prefix[int, bool]().attr)  # revealed: tuple[int, *tuple[tuple[bool], ...]]
# TODO: revealed: tuple[int, bool, str]
reveal_type(Prefix[int, bool, str]().attr)  # revealed: tuple[int, *tuple[tuple[bool, str], ...]]
# TODO: revealed: tuple[int, bool, str]
reveal_type(Prefix[int, *tuple[bool, str]]().attr)  # revealed: tuple[int, *tuple[tuple[bool, str], ...]]

# TODO: Should this raise an error?
# TODO: revealed: tuple[Unknown, *tuple[Unknown, ...]]
reveal_type(Prefix().attr)  # revealed: tuple[Unknown, *tuple[tuple[Unknown, ...], ...]]
```

```py
class Suffix(Generic[*Ts, T]):
    attr: tuple[*Ts, T]

# TODO: revealed: tuple[int]
reveal_type(Suffix[int]().attr)  # revealed: tuple[*tuple[tuple[()], ...], int]
# TODO: revealed: tuple[int, str]
reveal_type(Suffix[int, str]().attr)  # revealed: tuple[*tuple[tuple[int], ...], str]
# TODO: revealed: tuple[int, str, bool]
reveal_type(Suffix[int, str, bool]().attr)  # revealed: tuple[*tuple[tuple[int, str], ...], bool]
# TODO: revealed: tuple[int, str, bool]
reveal_type(Suffix[*tuple[int, str], bool]().attr)  # revealed: tuple[*tuple[tuple[int, str], ...], bool]

# TODO: Should this raise an error?
# TODO: revealed: tuple[*tuple[Unknown, ...], Unknown]
reveal_type(Suffix().attr)  # revealed: tuple[*tuple[tuple[Unknown, ...], ...], Unknown]
```

```py
class Between(Generic[T, *Ts, U]):
    attr: tuple[T, *Ts, U]

# TODO: revealed: tuple[int, str]
reveal_type(Between[int, str]().attr)  # revealed: tuple[int, *tuple[tuple[()], ...], str]
# TODO: revealed: tuple[int, bool, str]
reveal_type(Between[int, bool, str]().attr)  # revealed: tuple[int, *tuple[tuple[bool], ...], str]
# TODO: revealed: tuple[int, bool, bytes, str]
reveal_type(Between[int, bool, bytes, str]().attr)  # revealed: tuple[int, *tuple[tuple[bool, bytes], ...], str]
# TODO: revealed: tuple[int, bool, str]
reveal_type(Between[int, *tuple[bool], str]().attr)  # revealed: tuple[int, *tuple[tuple[bool], ...], str]

# TODO: revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
reveal_type(Between().attr)  # revealed: tuple[Unknown, *tuple[tuple[Unknown, ...], ...], Unknown]
# TODO: revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
# error: [invalid-type-arguments] "No type argument provided for required type variable `U` of class `Between`"
reveal_type(Between[int]().attr)  # revealed: tuple[Unknown, *tuple[tuple[Unknown, ...], ...], Unknown]
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
reveal_type(Positional(()))  # revealed: Positional[*tuple[Unknown, ...]]
# TODO: revealed: Positional[int, str]
reveal_type(Positional((1, "a")))  # revealed: Positional[int | str]

# TODO: revealed: Variadic[()]
reveal_type(Variadic())  # revealed: Variadic[*tuple[Unknown, ...]]
# TODO: revealed: Variadic[int, str]
reveal_type(Variadic(1, "a"))  # revealed: Variadic[int | str]

def _(i: int, s: str) -> None:
    # TODO: revealed: Positional[int, str]
    reveal_type(Positional((i, s)))  # revealed: Positional[int | str]
    # TODO: revealed: Variadic[int, str]
    reveal_type(Variadic(i, s))  # revealed: Variadic[int | str]
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
reveal_type(unspecified)  # revealed: Unspecified[*tuple[Unknown, ...]]
# TODO: revealed: tuple[Unknown, ...]
reveal_type(unspecified.attr)  # revealed: tuple[tuple[Unknown, ...], ...]
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

# error: [invalid-legacy-type-variable] "The default value for `TypeVarTuple` must be an unpacked tuple type or another TypeVarTuple"
InvalidDefault = TypeVarTuple("InvalidDefault", default=tuple[int, str])

class WithDefault(Generic[*Ts]):
    attr: tuple[*Ts]

# TODO: revealed: tuple[int, str]
reveal_type(WithDefault().attr)  # revealed: tuple[tuple[int, str], ...]
# TODO: revealed: tuple[bool, bytes]
reveal_type(WithDefault[bool, bytes]().attr)  # revealed: tuple[tuple[bool, bytes], ...]
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
reveal_type(WithBackportedDefault().attr)  # revealed: tuple[tuple[int, str], ...]
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
    # error: [invalid-type-arguments] "No type argument provided for required type variable `U`"
    a10: Between[int],
):
    # TODO: revealed: tuple[()]
    reveal_type(a1)  # revealed: tuple[tuple[()], ...]
    # TODO: revealed: tuple[int, str]
    reveal_type(a2)  # revealed: tuple[tuple[int, str], ...]
    # TODO: revealed: tuple[int, str]
    reveal_type(a3)  # revealed: tuple[int, *tuple[tuple[()], ...], str]
    # TODO: revealed: tuple[int, bool, str]
    reveal_type(a4)  # revealed: tuple[int, *tuple[tuple[bool], ...], str]
    # TODO: revealed: tuple[int, bool, bytes, str]
    reveal_type(a5)  # revealed: tuple[int, *tuple[tuple[bool, bytes], ...], str]
    # TODO: revealed: tuple[bool]
    reveal_type(a6)  # revealed: tuple[bool, *tuple[tuple[()], ...]]
    # TODO: revealed: tuple[bool, int, str]
    reveal_type(a7)  # revealed: tuple[bool, *tuple[tuple[int, str], ...]]
    # TODO: revealed: tuple[bool]
    reveal_type(a8)  # revealed: tuple[*tuple[tuple[()], ...], bool]
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(a9)  # revealed: tuple[*tuple[tuple[int, str], ...], bool]
    # TODO: revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
    reveal_type(a10)  # revealed: tuple[Unknown, *tuple[tuple[Unknown, ...], ...], Unknown]
```

### Unpacked tuple type arguments

```py
from typing import TypeVarTuple

Ts = TypeVarTuple("Ts")

Alias = tuple[int, *Ts]

def _(a1: Alias[*tuple[str, bool]], a2: Alias[*tuple[str, ...]]) -> None:
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(a1)  # revealed: tuple[int, *tuple[tuple[str, bool], ...]]
    # TODO: revealed: tuple[int, *tuple[str, ...]]
    reveal_type(a2)  # revealed: tuple[int, *tuple[tuple[str, ...], ...]]
```

### Unspecified alias type arguments

A bare variadic alias substitutes an unknown-length tuple of `Any`.

```py
from typing import Any, TypeVarTuple

Ts = TypeVarTuple("Ts")

Alias = tuple[bytes, *Ts]

def _(a1: Alias, a2: Alias[*tuple[Any, ...]]) -> None:
    # TODO: revealed: tuple[bytes, *tuple[Unknown, ...]]
    reveal_type(a1)  # revealed: tuple[bytes, *tuple[tuple[Unknown, ...], ...]]
    # TODO: revealed: tuple[bytes, *tuple[Any, ...]]
    reveal_type(a2)  # revealed: tuple[bytes, *tuple[tuple[Any, ...], ...]]
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
    reveal_type(f1)  # revealed: tuple[*tuple[tuple[int, ...], ...], int]
    # TODO: revealed: tuple[*tuple[int, ...], str]
    reveal_type(f2)  # revealed: tuple[*tuple[tuple[int, ...], ...], str]
    # TODO: revealed: tuple[int, *tuple[int, ...]]
    reveal_type(s1)  # revealed: tuple[int, *tuple[tuple[int, ...], ...]]
    # TODO: revealed: tuple[str, *tuple[int, ...]]
    reveal_type(s2)  # revealed: tuple[str, *tuple[tuple[int, ...], ...]]
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
    reveal_type(a1)  # revealed: tuple[bytes, *tuple[tuple[str, bool], ...]]
    # TODO: revealed: tuple[bytes, int, str, bool]
    reveal_type(a2)  # revealed: tuple[bytes, *tuple[tuple[int, *tuple[tuple[str, bool], ...]], ...]]
```

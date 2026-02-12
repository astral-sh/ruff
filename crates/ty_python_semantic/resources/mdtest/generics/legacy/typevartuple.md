# Legacy `TypeVarTuple`

## Definition

### Valid

```py
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple("Ts")
```

The name can also be provided as a keyword argument:

```py
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple(name="Ts")
```

### Must be directly assigned to a variable

```py
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple("Ts")
# error: [invalid-legacy-type-variable]
Ts1: TypeVarTuple = TypeVarTuple("Ts1")

# error: [invalid-legacy-type-variable]
tuple_with_tvt = ("foo", TypeVarTuple("Ts2"))
reveal_type(tuple_with_tvt[1])  # revealed: TypeVarTuple
```

### Name must match variable name

```py
from typing_extensions import TypeVarTuple

# error: [invalid-legacy-type-variable]
Ts = TypeVarTuple("Xs")
```

### Must have a name

```py
from typing_extensions import TypeVarTuple

# error: [invalid-legacy-type-variable]
Ts = TypeVarTuple()
```

### Name must be a string literal

```py
from typing_extensions import TypeVarTuple

def get_name() -> str:
    return "Ts"

# error: [invalid-legacy-type-variable]
Ts = TypeVarTuple(get_name())
```

### Only one positional argument

```py
from typing_extensions import TypeVarTuple

# error: [invalid-legacy-type-variable]
Ts = TypeVarTuple("Ts", int)
```

### No variadic arguments

```py
from typing_extensions import TypeVarTuple

args = ("Ts",)

# error: [invalid-legacy-type-variable]
Ts = TypeVarTuple(*args)

# error: [invalid-legacy-type-variable]
Xs = TypeVarTuple(**{"name": "Xs"})
```

### Name can't be given more than once

```py
from typing_extensions import TypeVarTuple

# error: [invalid-legacy-type-variable]
Ts = TypeVarTuple("Ts", name="Ts")
```

### Unknown keyword arguments

```py
from typing_extensions import TypeVarTuple

# error: [invalid-legacy-type-variable]
Ts = TypeVarTuple("Ts", invalid_keyword=True)
```

## Defaults

```py
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple("Ts", default=tuple[int, str])
```

## Usage in generic classes

### Specialization preserves variadic arguments

```py
from typing_extensions import Generic, TypeVarTuple, Unpack
from ty_extensions import generic_context

Ts = TypeVarTuple("Ts")

class Array(Generic[Unpack[Ts]]): ...

# revealed: ty_extensions.GenericContext[*Ts@Array]
reveal_type(generic_context(Array))

def check(a: Array[int, str, bytes]):
    reveal_type(a)  # revealed: Array[tuple[int, str, bytes]]

def check_single(a: Array[int]):
    reveal_type(a)  # revealed: Array[tuple[int]]
```

### `tuple[Unpack[Ts]]` in type annotations

The `Unpack[Ts]` subscript form and the `*Ts` starred form in tuple type annotations should both
produce a variadic tuple, not a fixed 1-element tuple.

```py
from typing_extensions import Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

def pass_through(*args: Unpack[Ts]) -> tuple[Unpack[Ts]]:
    return args

def forward(*args: Unpack[Ts]) -> None:
    inner: tuple[Unpack[Ts]] = args

def takes_args(args: tuple[Unpack[Ts]]) -> None: ...
def gives_args(*args: Unpack[Ts]) -> None:
    takes_args(args)

class Variadic(Generic[Unpack[Ts]]):
    def method(self, args: tuple[Unpack[Ts]]) -> None: ...

def accept_variadic(v: Variadic[int, str]):
    v.method((1, "hello"))

# `tuple[object, ...]` should be assignable to `tuple[*Ts]` since a TypeVarTuple's
# implicit element upper bound is `object`.
def returns_variadic() -> tuple[Unpack[Ts]] | None:
    args: tuple[object, ...] = ()
    return args
```

### Mixed TypeVar and TypeVarTuple

```py
from typing import TypeVar
from typing_extensions import Generic, TypeVarTuple, Unpack
from ty_extensions import generic_context

T = TypeVar("T")
Ts = TypeVarTuple("Ts")

class Pair(Generic[T, Unpack[Ts]]): ...

# revealed: ty_extensions.GenericContext[T@Pair, *Ts@Pair]
reveal_type(generic_context(Pair))

def check(a: Pair[int, str, bytes]):
    reveal_type(a)  # revealed: Pair[int, tuple[str, bytes]]
```

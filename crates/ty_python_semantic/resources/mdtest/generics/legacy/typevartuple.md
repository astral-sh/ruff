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

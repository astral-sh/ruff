# `unused-import` (`F401`)

```toml
target-version = "py315"

[lint]
select = ["F401"]
```

## Standard-library generic types

For standard-library generic types such as `slice` and `frozendict`, Ruff visits subscript arguments as
type expressions. Names in quoted annotations therefore mark their imports as used.

```py
from decimal import Decimal  # ok
from fractions import Fraction  # ok

DecimalSlice = slice["Decimal"]
FractionMap = frozendict[str, "Fraction"]
```

## Other subscription arguments

For an arbitrary subscription such as `foo["Decimal"]`, the string argument is not treated as a type
expression. The import of `Decimal` is therefore unused.

```py
from decimal import Decimal  # error: [unused-import]

from somewhere import foo

NotAnAlias = foo["Decimal"]
```

## Type parameters shadow imports

A reference to a type parameter does not use an import with the same name.

```py
from somewhere import T  # error: [unused-import]
from somewhere import U  # error: [unused-import]

def identity[T](value: T) -> T:
    return value
```

## Class type parameters shadow imports

```py
from somewhere import T  # error: [unused-import]

class Container[T]:
    value: T

    def get(self) -> T:
        return self.value
```

## Quoted type parameter annotations

Quoted annotations also refer to the enclosing type parameter.

```py
from somewhere import T  # error: [unused-import]

def identity[T](value: "T") -> "T":
    return value
```

## Imports used outside a generic definition

```py
from somewhere import T  # ok

def identity[T](value: T) -> T:
    return value

value: T
```

## Type parameter bounds use imports

```py
from somewhere import Bound  # ok

def identity[T: Bound](value: T) -> T:
    return value
```

## Variadic type parameters shadow imports

```py
from somewhere import P  # error: [unused-import]
from somewhere import Ts  # error: [unused-import]
from collections.abc import Callable

def call[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None:
    callback(*args, **kwargs)

def identity[*Ts](value: tuple[*Ts]) -> tuple[*Ts]:
    return value
```

## Future annotations with type parameters

```toml
target-version = "py312"

[lint]
select = ["F401"]
```

```py
from __future__ import annotations
from somewhere import T  # error: [unused-import]

def identity[T](value: T) -> T:
    return value
```

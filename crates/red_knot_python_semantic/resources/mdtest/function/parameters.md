# Function parameter types

Within a function scope, the declared type of each parameter is its annotated type (or Unknown if
not annotated). The initial inferred type is the union of the declared type with the type of the
default value expression (if any). If both are fully static types, this union should simplify to the
annotated type (since the default value type must be assignable to the annotated type, and for fully
static types this means subtype-of, which simplifies in unions). But if the annotated type is
Unknown or another non-fully-static type, the default value type may still be relevant as lower
bound.

The variadic parameter is a variadic tuple of its annotated type; the variadic-keywords parameter is
a dictionary from strings to its annotated type.

## Parameter kinds

```py
from typing import Literal

def f(a, b: int, c=1, d: int = 2, /, e=3, f: Literal[4] = 4, *args: object, g=5, h: Literal[6] = 6, **kwargs: str):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: int
    reveal_type(c)  # revealed: Unknown | Literal[1]
    reveal_type(d)  # revealed: int
    reveal_type(e)  # revealed: Unknown | Literal[3]
    reveal_type(f)  # revealed: Literal[4]
    reveal_type(g)  # revealed: Unknown | Literal[5]
    reveal_type(h)  # revealed: Literal[6]

    # TODO: should be `tuple[object, ...]` (needs generics)
    reveal_type(args)  # revealed: tuple

    # TODO: should be `dict[str, str]` (needs generics)
    reveal_type(kwargs)  # revealed: dict
```

## Unannotated variadic parameters

...are inferred as tuple of Unknown or dict from string to Unknown.

```py
def g(*args, **kwargs):
    # TODO: should be `tuple[Unknown, ...]` (needs generics)
    reveal_type(args)  # revealed: tuple

    # TODO: should be `dict[str, Unknown]` (needs generics)
    reveal_type(kwargs)  # revealed: dict
```

## Annotation is present but not a fully static type

The default value type should be a lower bound on the inferred type.

```py
from typing import Any

def f(x: Any = 1):
    reveal_type(x)  # revealed: Any | Literal[1]
```

## Default value type must be assignable to annotated type

The default value type must be assignable to the annotated type. If not, we emit a diagnostic, and
fall back to inferring the annotated type, ignoring the default value type.

```py
# error: [invalid-parameter-default]
def f(x: int = "foo"):
    reveal_type(x)  # revealed: int

# The check is assignable-to, not subtype-of, so this is fine:
from typing import Any

def g(x: Any = "foo"):
    reveal_type(x)  # revealed: Any | Literal["foo"]
```

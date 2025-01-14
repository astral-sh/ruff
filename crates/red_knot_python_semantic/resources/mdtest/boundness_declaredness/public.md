# Boundness and declaredness: public symbols

This document demonstrates how type-inference and diagnostics works for public symbols. We test the
whole matrix of possible boundness and declaredness states. The current behavior is as follows (as
demonstrated by the tests below):

| **Public type**  | declared     | possibly-undeclared        | undeclared   |
| ---------------- | ------------ | -------------------------- | ------------ |
| bound            | `T_declared` | `T_declared \| T_inferred` | `T_inferred` |
| possibly-unbound | `T_declared` | `T_declared \| T_inferred` | `T_inferred` |
| unbound          | `T_declared` | `T_declared`               | `Unknown`    |

| **Diagnostic**   | declared | possibly-undeclared       | undeclared          |
| ---------------- | -------- | ------------------------- | ------------------- |
| bound            |          |                           |                     |
| possibly-unbound |          | `possibly-unbound-import` |                     |
| unbound          |          |                           | `unresolved-import` |

Note: Some of this behavior is questionable and might change in the future. See the TODOs in
`symbol_by_id` (`types.rs`) and [this issue](https://github.com/astral-sh/ruff/issues/14297) for
more information.

## Declared

### Declared and bound

If a symbol has a declared type (`int`), we use that even if there is a more precise inferred type
(`Literal[1]`), or a conflicting inferred type (`Literal[2]`).

```py path=mod.py
x: int = 1

# error: [invalid-assignment]
y: str = 2
```

```py
from mod import x, y

reveal_type(x)  # revealed: int
reveal_type(y)  # revealed: str
```

### Declared and possibly unbound

If a symbol is declared and *possibly* unbound, we also trust that other module and use the declared
type without raising an error.

```py path=mod.py
def flag() -> bool: ...

x: int
y: str
if flag:
    x = 1
    # error: [invalid-assignment]
    y = 2
```

```py
from mod import x, y

reveal_type(x)  # revealed: int
reveal_type(y)  # revealed: str
```

### Declared and unbound

If a symbol is declared but unbound, we do not raise an error. We trust that this symbol is
available somehow and simply use the declared type.

```py path=mod.py
x: int
```

```py
from mod import x

reveal_type(x)  # revealed: int
```

## Possibly undeclared

### Possibly undeclared and bound

If a symbol is possibly undeclared but definitely bound, we use the union of the declared and
inferred types.

```py path=mod.py
from typing import Any

def flag() -> bool: ...

x = 1
y = 2
if flag():
    x: Any
    # error: [invalid-declaration]
    y: str
```

```py
from mod import x, y

reveal_type(x)  # revealed: Literal[1] | Any
reveal_type(y)  # revealed: Literal[2] | Unknown
```

### Possibly undeclared and possibly unbound

If a symbol is possibly undeclared and possibly unbound, we also use the union of the declared and
inferred types. This case is interesting because the "possibly declared" definition might not be the
same as the "possibly bound" definition (symbol `y`).

```py path=mod.py
def flag() -> bool: ...

if flag():
    x: Any = 1
    y = 2
else:
    y: str
```

```py
# error: [possibly-unbound-import]
# error: [possibly-unbound-import]
from mod import x, y

reveal_type(x)  # revealed: Literal[1] | Any
reveal_type(y)  # revealed: Literal[2] | str
```

### Possibly undeclared and unbound

If a symbol is possibly undeclared and definitely unbound, we also don't raise an error:

```py path=mod.py
def flag() -> bool: ...

if flag():
    x: int
```

```py
from mod import x

reveal_type(x)  # revealed: int
```

## Undeclared

### Undeclared but bound

We use the inferred type as the public type, if a symbol has no declared type.

```py path=mod.py
x = 1
```

```py
from mod import x

reveal_type(x)  # revealed: Literal[1]
```

### Undeclared and possibly unbound

If a symbol is undeclared and *possibly* unbound, we trust that other module (the control flow path
that leads to unbound could be ruled out for some reason that we can't see statically) and pretend
that the symbol is actually bound. We do not raise an error.

```py path=mod.py
def flag() -> bool: ...

if flag:
    x = 1
```

```py
from mod import x

reveal_type(x)  # revealed: Literal[1]
```

### Undeclared and unbound

If a symbol is undeclared *and* unbound, we infer `Unknown` and raise an error.

```py path=mod.py
if False:
    x: int = 1
```

```py
# error: [unresolved-import]
from mod import x

reveal_type(x)  # revealed: Unknown
```

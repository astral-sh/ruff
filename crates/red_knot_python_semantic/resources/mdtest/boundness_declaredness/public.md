# Boundness and declaredness: public uses

This document demonstrates how type-inference and diagnostics works for *public* uses of a symbol,
that is, a use of a symbol from another scope. If a symbol has a declared type in its local scope
(e.g. `int`), we use that as the symbol's "public type" (the type of the symbol from the perspective
of other scopes) even if there is a more precise local inferred type for the symbol (`Literal[1]`).

If a symbol has no declared type, we use the union of `Unknown` with the inferred type as the public
type. If there is no declaration, then the symbol can be reassigned to any type from another scope;
the union with `Unknown` reflects that its type must at least be as large as the type of the
assigned value, but could be arbitrarily larger.

We test the whole matrix of possible boundness and declaredness states. The current behavior is
summarized in the following table, while the tests below demonstrate each case. Note that some of
this behavior is questionable and might change in the future. See the TODOs in `symbol_by_id`
(`types.rs`) and [this issue](https://github.com/astral-sh/ruff/issues/14297) for more information.
In particular, we should raise errors in the "possibly-undeclared-and-unbound" as well as the
"undeclared-and-possibly-unbound" cases (marked with a "?").

| **Public type**  | declared     | possibly-undeclared        | undeclared              |
| ---------------- | ------------ | -------------------------- | ----------------------- |
| bound            | `T_declared` | `T_declared \| T_inferred` | `Unknown \| T_inferred` |
| possibly-unbound | `T_declared` | `T_declared \| T_inferred` | `Unknown \| T_inferred` |
| unbound          | `T_declared` | `T_declared`               | `Unknown`               |

| **Diagnostic**   | declared | possibly-undeclared       | undeclared          |
| ---------------- | -------- | ------------------------- | ------------------- |
| bound            |          |                           |                     |
| possibly-unbound |          | `possibly-unbound-import` | ?                   |
| unbound          |          | ?                         | `unresolved-import` |

## Declared

### Declared and bound

If a symbol has a declared type (`int`), we use that even if there is a more precise inferred type
(`Literal[1]`), or a conflicting inferred type (`Literal[2]`):

`mod.py`:

```py
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

If a symbol is declared and *possibly* unbound, we trust that other module and use the declared type
without raising an error.

`mod.py`:

```py
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

Similarly, if a symbol is declared but unbound, we do not raise an error. We trust that this symbol
is available somehow and simply use the declared type.

`mod.py`:

```py
x: int
```

```py
from mod import x

reveal_type(x)  # revealed: int
```

## Possibly undeclared

### Possibly undeclared and bound

If a symbol is possibly undeclared but definitely bound, we use the union of the declared and
inferred types:

`mod.py`:

```py
from typing import Any

def flag() -> bool: ...

x = 1
y = 2
z = 3
if flag():
    x: int
    y: Any
    # error: [invalid-declaration]
    z: str
```

```py
from mod import x, y, z

reveal_type(x)  # revealed: int
reveal_type(y)  # revealed: Literal[2] | Any
reveal_type(z)  # revealed: Literal[3] | Unknown

# External modifications of `x` that violate the declared type are not allowed:
# error: [invalid-assignment]
x = None
```

### Possibly undeclared and possibly unbound

If a symbol is possibly undeclared and possibly unbound, we also use the union of the declared and
inferred types. This case is interesting because the "possibly declared" definition might not be the
same as the "possibly bound" definition (symbol `y`). Note that we raise a `possibly-unbound-import`
error for both `x` and `y`:

`mod.py`:

```py
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

# External modifications of `y` that violate the declared type are not allowed:
# error: [invalid-assignment]
y = None
```

### Possibly undeclared and unbound

If a symbol is possibly undeclared and definitely unbound, we currently do not raise an error. This
seems inconsistent when compared to the case just above.

`mod.py`:

```py
def flag() -> bool: ...

if flag():
    x: int
```

```py
# TODO: this should raise an error. Once we fix this, update the section description and the table
# on top of this document.
from mod import x

reveal_type(x)  # revealed: int

# External modifications to `x` that violate the declared type are not allowed:
# error: [invalid-assignment]
x = None
```

## Undeclared

### Undeclared but bound

`mod.py`:

```py
x = 1
```

```py
from mod import x

reveal_type(x)  # revealed: Unknown | Literal[1]

# All external modifications of `x` are allowed:
x = None
```

### Undeclared and possibly unbound

If a symbol is undeclared and *possibly* unbound, we currently do not raise an error. This seems
inconsistent when compared to the "possibly-undeclared-and-possibly-unbound" case.

`mod.py`:

```py
def flag() -> bool: ...

if flag:
    x = 1
```

```py
# TODO: this should raise an error. Once we fix this, update the section description and the table
# on top of this document.
from mod import x

reveal_type(x)  # revealed: Unknown | Literal[1]

# All external modifications of `x` are allowed:
x = None
```

### Undeclared and unbound

If a symbol is undeclared *and* unbound, we infer `Unknown` and raise an error.

`mod.py`:

```py
if False:
    x: int = 1
```

```py
# error: [unresolved-import]
from mod import x

reveal_type(x)  # revealed: Unknown

# Modifications allowed in this case:
x = None
```

# Boundness and declaredness: public uses

This document demonstrates how type-inference and diagnostics work for *public* uses of a symbol,
that is, a use of a symbol from another scope. If a symbol has a declared type in its local scope
(e.g. `int`), we use that as the symbol's "public type" (the type of the symbol from the perspective
of other scopes). If there is an inferred type in addition (i.e. if we also see bindings for this
symbol, not just declarations), we use `T_decl | T_decl & T_inf` as the public type, which
simplifies to `T_decl` for `T_inf = Unknown` (the unbound case).

[TODO: more explanation]

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

| **Public type**  | declared                   | possibly-undeclared | undeclared         |
| ---------------- | -------------------------- | ------------------- | ------------------ |
| bound            | `T_decl \| T_decl & T_inf` | `T_decl \| T_inf`   | `Unknown \| T_inf` |
| possibly-unbound | `T_decl \| T_decl & T_inf` | `T_decl \| T_inf`   | `Unknown \| T_inf` |
| unbound          | `T_decl`                   | `T_decl`            | `Unknown`          |

| **Diagnostic**   | declared | possibly-undeclared       | undeclared          |
| ---------------- | -------- | ------------------------- | ------------------- |
| bound            |          |                           |                     |
| possibly-unbound |          | `possibly-unbound-import` | ?                   |
| unbound          |          | ?                         | `unresolved-import` |

## Declared

### Declared and bound

If a symbol has a declared type (`int`), we use that even if there is a more precise inferred type
(`Literal[1]`), or a conflicting inferred type (`Literal[2]`):

```py path=mod.py
from typing import Any

def any() -> Any: ...

a: int = 1

# error: [invalid-assignment]
b: str = 2

c: Any = 3

d: int = any()
```

```py
from mod import a, b, c, d

reveal_type(a)  # revealed: int
reveal_type(b)  # revealed: str
reveal_type(c)  # revealed: Any | Literal[3]
reveal_type(d)  # revealed: int
```

### Declared and possibly unbound

If a symbol is declared and *possibly* unbound, we trust that other module and use the declared type
without raising an error.

```py path=mod.py
from typing import Any

def any() -> Any: ...
def flag() -> bool: ...

a: int
b: str
c: Any
d: int

if flag:
    a = 1
    # error: [invalid-assignment]
    b = 2
    c = 3
    d = any()
```

```py
from mod import a, b, c, d

reveal_type(a)  # revealed: int
reveal_type(b)  # revealed: str
reveal_type(c)  # revealed: Any | Literal[3]
reveal_type(d)  # revealed: int
```

### Declared and unbound

Similarly, if a symbol is declared but unbound, we do not raise an error. We trust that this symbol
is available somehow and simply use the declared type.

```py path=mod.py
from typing import Any

a: int
b: Any
```

```py
from mod import a, b

reveal_type(a)  # revealed: int
reveal_type(b)  # revealed: Any
```

## Possibly undeclared

### Possibly undeclared and bound

If a symbol is possibly undeclared but definitely bound, we use the union of the declared and
inferred types:

```py path=mod.py
from typing import Any

def flag() -> bool: ...

a = 1
b = 2
c = 3
if flag():
    a: int
    b: Any
    # error: [invalid-declaration]
    c: str
```

```py
from mod import a, b, c

reveal_type(a)  # revealed: int
reveal_type(b)  # revealed: Literal[2] | Any
reveal_type(c)  # revealed: Literal[3] | Unknown

# External modifications of `a` that violate the declared type are not allowed:
# error: [invalid-assignment]
a = None
```

### Possibly undeclared and possibly unbound

If a symbol is possibly undeclared and possibly unbound, we also use the union of the declared and
inferred types. This case is interesting because the "possibly declared" definition might not be the
same as the "possibly bound" definition (symbol `b`). Note that we raise a `possibly-unbound-import`
error for both `a` and `b`:

```py path=mod.py
def flag() -> bool: ...

if flag():
    a: Any = 1
    b = 2
else:
    b: str
```

```py
# error: [possibly-unbound-import]
# error: [possibly-unbound-import]
from mod import a, b

reveal_type(a)  # revealed: Literal[1] | Any
reveal_type(b)  # revealed: Literal[2] | str

# External modifications of `b` that violate the declared type are not allowed:
# error: [invalid-assignment]
b = None
```

### Possibly undeclared and unbound

If a symbol is possibly undeclared and definitely unbound, we currently do not raise an error. This
seems inconsistent when compared to the case just above.

```py path=mod.py
def flag() -> bool: ...

if flag():
    a: int
```

```py
# TODO: this should raise an error. Once we fix this, update the section description and the table
# on top of this document.
from mod import a

reveal_type(a)  # revealed: int

# External modifications to `a` that violate the declared type are not allowed:
# error: [invalid-assignment]
a = None
```

## Undeclared

### Undeclared but bound

```py path=mod.py
a = 1
```

```py
from mod import a

reveal_type(a)  # revealed: Unknown | Literal[1]

# All external modifications of `a` are allowed:
a = None
```

### Undeclared and possibly unbound

If a symbol is undeclared and *possibly* unbound, we currently do not raise an error. This seems
inconsistent when compared to the "possibly-undeclared-and-possibly-unbound" case.

```py path=mod.py
def flag() -> bool: ...

if flag:
    a = 1
```

```py
# TODO: this should raise an error. Once we fix this, update the section description and the table
# on top of this document.
from mod import a

reveal_type(a)  # revealed: Unknown | Literal[1]

# All external modifications of `a` are allowed:
a = None
```

### Undeclared and unbound

If a symbol is undeclared *and* unbound, we infer `Unknown` and raise an error.

```py path=mod.py
if False:
    a: int = 1
```

```py
# error: [unresolved-import]
from mod import a

reveal_type(a)  # revealed: Unknown

# Modifications allowed in this case:
a = None
```

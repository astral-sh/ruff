# Boundness and declaredness: public uses

This document demonstrates how type-inference and diagnostics work for *public* uses of a symbol,
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
| possibly-unbound |          | `possibly-missing-import` | ?                   |
| unbound          |          | ?                         | `unresolved-import` |

## Declared

### Declared and bound

If a symbol has a declared type (`int`), we use that even if there is a more precise inferred type
(`Literal[1]`), or a conflicting inferred type (`str` vs. `Literal[2]` below):

```py
from typing import Any

def any() -> Any: ...

class Public:
    a: int = 1
    b: str = 2  # error: [invalid-assignment]
    c: Any = 3
    d: int = any()

reveal_type(Public.a)  # revealed: int
reveal_type(Public.b)  # revealed: str
reveal_type(Public.c)  # revealed: Any
reveal_type(Public.d)  # revealed: int
```

### Declared and possibly unbound

If a symbol is declared and *possibly* unbound, we trust the declared type without raising an error.

```py
from typing import Any

def any() -> Any: ...
def flag() -> bool:
    return True

class Public:
    a: int
    b: str
    c: Any
    d: int

    if flag:
        a = 1
        b = 2  # error: [invalid-assignment]
        c = 3
        d = any()

reveal_type(Public.a)  # revealed: int
reveal_type(Public.b)  # revealed: str
reveal_type(Public.c)  # revealed: Any
reveal_type(Public.d)  # revealed: int
```

### Declared and unbound

Similarly, if a symbol is declared but unbound, we do not raise an error. We trust that this symbol
is available somehow and simply use the declared type.

```py
from typing import Any

class Public:
    a: int
    b: Any

reveal_type(Public.a)  # revealed: int
reveal_type(Public.b)  # revealed: Any
```

## Possibly undeclared

### Possibly undeclared and bound

If a symbol is possibly undeclared but definitely bound, we use the union of the declared and
inferred types:

```py
from typing import Any

def any() -> Any: ...
def flag() -> bool:
    return True

class Public:
    a = 1
    b = 2
    c = 3
    d = any()
    if flag():
        a: int
        b: Any
        c: str  # error: [invalid-declaration]
        d: int

reveal_type(Public.a)  # revealed: int
reveal_type(Public.b)  # revealed: Literal[2] | Any
reveal_type(Public.c)  # revealed: Literal[3] | Unknown
reveal_type(Public.d)  # revealed: Any | int

# External modifications of `a` that violate the declared type are not allowed:
# error: [invalid-assignment]
Public.a = None
```

### Possibly undeclared and possibly unbound

If a symbol is possibly undeclared and possibly unbound, we also use the union of the declared and
inferred types. This case is interesting because the "possibly declared" definition might not be the
same as the "possibly bound" definition (symbol `b`). Note that we raise a `possibly-missing-import`
error for both `a` and `b`:

```py
from typing import Any

def flag() -> bool:
    return True

class Public:
    if flag():
        a: Any = 1
        b = 2
    else:
        b: str

# error: [possibly-missing-attribute]
reveal_type(Public.a)  # revealed: Literal[1] | Any
# error: [possibly-missing-attribute]
reveal_type(Public.b)  # revealed: Literal[2] | str

# External modifications of `b` that violate the declared type are not allowed:
# error: [possibly-missing-attribute]
# error: [invalid-assignment]
Public.b = None
```

### Possibly undeclared and unbound

If a symbol is possibly undeclared and definitely unbound, we currently do not raise an error. This
seems inconsistent when compared to the case just above.

```py
def flag() -> bool:
    return True

class Public:
    if flag():
        a: int

# TODO: this should raise an error. Once we fix this, update the section description and the table
# on top of this document.
reveal_type(Public.a)  # revealed: int

# External modifications to `a` that violate the declared type are not allowed:
# error: [invalid-assignment]
Public.a = None
```

## Undeclared

### Undeclared but bound

If a symbol is *undeclared*, we use the union of `Unknown` with the inferred type. Note that we
treat this case differently from the case where a symbol is implicitly declared with `Unknown`,
possibly due to the usage of an unknown name in the annotation:

```py
class Public:
    # Undeclared:
    a = 1

    # Implicitly declared with `Unknown`, due to the usage of an unknown name in the annotation:
    b: SomeUnknownName = 1  # error: [unresolved-reference]

reveal_type(Public.a)  # revealed: Unknown | Literal[1]
reveal_type(Public.b)  # revealed: Unknown

# All external modifications of `a` are allowed:
Public.a = None
```

### Undeclared and possibly unbound

If a symbol is undeclared and *possibly* unbound, we currently do not raise an error. This seems
inconsistent when compared to the "possibly-undeclared-and-possibly-unbound" case.

```py
def flag() -> bool:
    return True

class Public:
    if flag:
        a = 1
        b: SomeUnknownName = 1  # error: [unresolved-reference]

# TODO: these should raise an error. Once we fix this, update the section description and the table
# on top of this document.
reveal_type(Public.a)  # revealed: Unknown | Literal[1]
reveal_type(Public.b)  # revealed: Unknown

# All external modifications of `a` are allowed:
Public.a = None
```

### Undeclared and unbound

If a symbol is undeclared *and* unbound, we infer `Unknown` and raise an error.

```py
class Public:
    if False:
        a: int = 1

# error: [unresolved-attribute]
reveal_type(Public.a)  # revealed: Unknown

# Modification attempts yield an error:
# error: [unresolved-attribute]
Public.a = None
```

## In stub files

In stub files, we have a minor modification to the rules above: we do not union with `Unknown` for
undeclared symbols.

### Undeclared and bound

`mod.pyi`:

```pyi
MyInt = int

class C:
    MyStr = str
```

```py
from mod import MyInt, C

reveal_type(MyInt)  # revealed: <class 'int'>
reveal_type(C.MyStr)  # revealed: <class 'str'>
```

### Undeclared and possibly unbound

`mod.pyi`:

```pyi
def flag() -> bool:
    return True

if flag():
    MyInt = int

    class C:
        MyStr = str
```

```py
# error: [possibly-missing-import]
# error: [possibly-missing-import]
from mod import MyInt, C

reveal_type(MyInt)  # revealed: <class 'int'>
reveal_type(C.MyStr)  # revealed: <class 'str'>
```

### Undeclared and unbound

`mod.pyi`:

```pyi
if False:
    MyInt = int
```

```py
# error: [unresolved-import]
from mod import MyInt

reveal_type(MyInt)  # revealed: Unknown
```

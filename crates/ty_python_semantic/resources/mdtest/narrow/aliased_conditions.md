# Narrowing with aliased conditions

Narrowing is supported when a narrowing expression is stored in an intermediate variable (an
"aliased conditional expression") and that variable is later used as a condition.

## Basic `is None` alias

```py
def _(x: int | None):
    is_none = x is None
    if is_none:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: int

def _(x: int | None):
    is_none: bool = x is None
    if is_none:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: int

def _(x: str | int | None):
    is_none = x is None
    is_int = isinstance(x, int)
    if is_none:
        reveal_type(x)  # revealed: None
    if is_int:
        reveal_type(x)  # revealed: int
    if is_none or is_int:
        reveal_type(is_none)  # revealed: bool
        reveal_type(x)  # revealed: None | int
    if is_none and is_int:
        reveal_type(is_none)  # revealed: Literal[True]
        reveal_type(x)  # revealed: Never
    if not (is_none or is_int):
        reveal_type(is_none)  # revealed: Literal[False]
        reveal_type(x)  # revealed: str
```

## Negated alias with `not`

```py
def _(x: int | None):
    is_none = x is None
    if not is_none:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: None
```

## `is not None` alias

```py
def _(x: int | None):
    is_not_none = x is not None
    if is_not_none:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: None
```

## `isinstance` alias

```py
def _(x: int | str):
    is_int = isinstance(x, int)
    if is_int:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: str
```

## Equality comparisons

```py
from typing import Literal

def _(x: Literal[1, 2]):
    is_one = x == 1
    if is_one:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: Literal[2]
```

## Attribute access alias

```py
class A:
    x: int | None
    b: bool

def _(a: A):
    is_none = a.x is None
    if is_none:
        reveal_type(a.x)  # revealed: None
    else:
        reveal_type(a.x)  # revealed: int

    class Inner:
        if is_none:
            reveal_type(a.x)  # revealed: None

def _(a: A):
    is_none = a.x is None
    a.x = 1
    if is_none:
        reveal_type(a.x)  # revealed: Literal[1]

def _(a: A):
    # Attribute targets are not treated as aliases.
    a.b = a.x is None
    if a.b:
        reveal_type(a.x)  # revealed: int | None
    else:
        reveal_type(a.x)  # revealed: int | None
```

## Subscript access alias

```py
def _(l: list[int | None]):
    is_none = l[0] is None
    if is_none:
        reveal_type(l[0])  # revealed: None
    else:
        reveal_type(l[0])  # revealed: int

    class Inner:
        if is_none:
            reveal_type(l[0])  # revealed: None

def _(l: list[int | None], lb: list[bool]):
    # Same as above: subscript targets are not treated as aliases.
    lb[0] = l[0] is None
    if lb[0]:
        reveal_type(l[0])  # revealed: int | None
    else:
        reveal_type(l[0])  # revealed: int | None
```

## Narrowed variable reassigned invalidates alias

If the narrowed variable is reassigned between the alias and its use as a condition, the alias must
not be followed.

```py
def _(x: int | None, cond: bool):
    is_none = x is None
    if cond:
        x = 1
    if is_none:
        reveal_type(x)  # revealed: int | None

    is_none = x is None
    if is_none:
        reveal_type(x)  # revealed: None
```

## Alias variable reassigned invalidates alias

If the alias variable itself is reassigned, it no longer represents the original check.

```py
def _(x: int | None):
    is_none = x is None
    is_none = True
    if is_none:
        reveal_type(x)  # revealed: int | None

    is_none = x is None
    if is_none:
        reveal_type(x)  # revealed: None
```

## Non-narrowing assignment is not treated as alias

A regular function call result is not a narrowing expression, so only truthiness narrowing applies
to the variable itself.

```py
def some_function(x: object) -> bool:
    return True

def _(x: int | None):
    result = some_function(x)
    if result:
        reveal_type(x)  # revealed: int | None
```

## Nested scope can preserve alias

Aliases remain valid inside nested scopes. For eager scopes (class bodies, comprehensions) they are
evaluated inline. For lazy scopes (inner functions), the narrowing evaluator uses lazy snapshots to
track whether the narrowed variable was reassigned, so alias-based narrowing works the same as
direct narrowing across scope boundaries.

```py
def _(x: int | None):
    is_none = x is None

    if is_none:
        reveal_type(x)  # revealed: None

    class Inner:
        if is_none:
            reveal_type(x)  # revealed: None

        def inner():
            if is_none:
                reveal_type(x)  # revealed: None

    def inner2():
        if is_none:
            reveal_type(x)  # revealed: None

        class Inner2:
            if is_none:
                reveal_type(x)  # revealed: None

class A:
    x: int | None

def _(a: A):
    a = A()
    a.x = None
    is_none = a.x is None

    if is_none:
        reveal_type(a.x)  # revealed: None

    class Inner:
        if is_none:
            reveal_type(a.x)  # revealed: None

        def inner():
            if is_none:
                reveal_type(a.x)  # revealed: None

    def inner2():
        if is_none:
            reveal_type(a.x)  # revealed: None

        class Inner2:
            if is_none:
                reveal_type(a.x)  # revealed: None
```

## Cross-scope invalidation: narrowed variable reassigned

If the narrowed variable is reassigned inside an eager scope, the alias is invalidated within that
scope.

```py
def _(x: int | None):
    is_none = x is None

    class Inner:
        x = 42
        x = 43
        if is_none:
            reveal_type(x)  # revealed: Literal[43]

        def f():
            reveal_type(x)  # revealed: int | None
            if is_none:
                reveal_type(x)  # revealed: None

        class Inner2:
            if is_none:
                # `x` here refers to the function scope variable, not the class-scope `x`.
                # Python's name resolution skips class scopes for nested scopes, so the alias
                # remains valid.
                reveal_type(x)  # revealed: None

    if is_none:
        reveal_type(x)  # revealed: None
```

The same applies to a lazy scope:

```py
def _(x: int | None):
    is_none = x is None

    def inner():
        nonlocal x
        x = 42
        if is_none:
            reveal_type(x)  # revealed: Literal[42]

    # TODO: should be `int | None`
    # We don't yet track that `inner()` can modify `x` via `nonlocal`.
    # (https://github.com/astral-sh/ty/issues/2731)
    if is_none:
        reveal_type(x)  # revealed: None

def _(x: int | None):
    is_none = x is None

    def inner():
        if is_none:
            reveal_type(x)  # revealed: int | None

        def inner2():
            if is_none:
                reveal_type(x)  # revealed: int | None

    x = 42

    inner()
```

## Cross-scope invalidation: alias variable reassigned

If the alias variable itself is reassigned inside an eager scope, the alias is invalidated within
that scope.

```py
def _(x: int | None):
    is_none = x is None

    class Inner:
        is_none = True
        if is_none:
            reveal_type(x)  # revealed: int | None

        class Inner2:
            # `is_none` here refers to the function scope variable, not the class-scope
            # `is_none = True`. Python's name resolution skips class scopes for nested
            # scopes, so the alias remains valid.
            if is_none:
                reveal_type(x)  # revealed: None

    if is_none:
        reveal_type(x)  # revealed: None
```

The same applies to a lazy scope:

```py
def _(x: int | None):
    is_none = x is None

    def inner():
        nonlocal is_none
        is_none = True
        if is_none:
            reveal_type(x)  # revealed: int | None

    inner()

    # TODO: should be `int | None`
    # We don't yet track that `inner()` can modify `is_none` via `nonlocal`.
    # (https://github.com/astral-sh/ty/issues/2731)
    if is_none:
        reveal_type(x)  # revealed: None

def _(x: int | None):
    is_none = x is None

    def inner():
        if is_none:
            reveal_type(x)  # revealed: int | None

        def inner2():
            if is_none:
                reveal_type(x)  # revealed: int | None

    is_none = True

    inner()
```

## Chained alias

```py
def _(x: int | None):
    is_none = x is None
    is_none_alias = is_none
    if is_none_alias:
        reveal_type(x)  # revealed: None

    class Inner:
        if is_none_alias:
            reveal_type(x)  # revealed: None

    def inner():
        if is_none_alias:
            reveal_type(x)  # revealed: None

def _(x: int | None):
    is_none = x is None
    is_none_alias = is_none

    x = 42

    if is_none_alias:
        reveal_type(x)  # revealed: Literal[42]
    if is_none:
        reveal_type(x)  # revealed: Literal[42]

    class Inner:
        if is_none_alias:
            reveal_type(x)  # revealed: Literal[42]
        if is_none:
            reveal_type(x)  # revealed: Literal[42]

    def inner():
        x = 42
        if is_none_alias:
            reveal_type(x)  # revealed: Literal[42]
        if is_none:
            reveal_type(x)  # revealed: Literal[42]

def _(x: int | None):
    is_none = x is None
    is_none_alias = is_none

    class Inner:
        is_none_alias = True
        if is_none_alias:
            reveal_type(x)  # revealed: int | None
        if is_none:
            reveal_type(x)  # revealed: None

        class Inner2:
            if is_none_alias:
                reveal_type(x)  # revealed: None
            if is_none:
                reveal_type(x)  # revealed: None

    class Inner2:
        is_none = True
        if is_none_alias:
            reveal_type(x)  # revealed: None
        if is_none:
            reveal_type(x)  # revealed: int | None

        class Inner3:
            if is_none_alias:
                reveal_type(x)  # revealed: None
            if is_none:
                reveal_type(x)  # revealed: None

    def inner():
        is_none_alias = True
        if is_none_alias:
            reveal_type(x)  # revealed: int | None
        if is_none:
            reveal_type(x)  # revealed: None

        def inner2():
            if is_none_alias:
                reveal_type(x)  # revealed: int | None
            if is_none:
                reveal_type(x)  # revealed: None

    def inner2():
        is_none = True
        if is_none_alias:
            reveal_type(x)  # revealed: None
        if is_none:
            reveal_type(x)  # revealed: int | None

        def inner3():
            if is_none_alias:
                reveal_type(x)  # revealed: None
            if is_none:
                reveal_type(x)  # revealed: int | None
```

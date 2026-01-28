# Narrowing using `getattr()`

The builtin function `getattr()` with a string-literal attribute name can be used to narrow the type
of the corresponding attribute based on truthiness.

## Basic truthiness narrowing

When `getattr(obj, "attr")` is used as a condition, the attribute type is narrowed based on
truthiness:

```py
class C:
    flag: bool

def _(x: C):
    if getattr(x, "flag"):
        reveal_type(x.flag)  # revealed: Literal[True]
    else:
        reveal_type(x.flag)  # revealed: Literal[False]
```

## Narrowing with optional attributes

When an attribute might be `None`, `getattr` truthiness can narrow it:

```py
class C:
    value: str | None

def _(x: C):
    if getattr(x, "value"):
        reveal_type(x.value)  # revealed: str & ~AlwaysFalsy
    else:
        reveal_type(x.value)  # revealed: (str & ~AlwaysTruthy) | None
```

## Nested attribute access

`getattr` with nested objects narrows the leaf attribute:

```py
class Inner:
    flag: bool

class Outer:
    inner: Inner

def _(x: Outer):
    if getattr(x.inner, "flag"):
        reveal_type(x.inner.flag)  # revealed: Literal[True]
    else:
        reveal_type(x.inner.flag)  # revealed: Literal[False]
```

## Subscript access

`getattr` also works with subscript expressions:

```py
class C:
    flag: bool

def _(x: list[C]):
    if getattr(x[0], "flag"):
        reveal_type(x[0].flag)  # revealed: Literal[True]
    else:
        reveal_type(x[0].flag)  # revealed: Literal[False]
```

## No narrowing for non-identifier strings

When the attribute name is not a valid identifier, no narrowing occurs:

```py
class C:
    flag: bool

def _(x: C):
    if getattr(x, "not-an-identifier"):
        reveal_type(x.flag)  # revealed: bool
    else:
        reveal_type(x.flag)  # revealed: bool
```

## No narrowing for non-literal strings

When the attribute name is not a string literal, no narrowing occurs:

```py
class C:
    flag: bool

def _(x: C, attr: str):
    if getattr(x, attr):
        reveal_type(x.flag)  # revealed: bool
    else:
        reveal_type(x.flag)  # revealed: bool
```

## Getattr with default value

The 3-argument form `getattr(obj, "attr", default)` also supports narrowing:

```py
class C:
    flag: bool

def _(x: C):
    if getattr(x, "flag", False):
        reveal_type(x.flag)  # revealed: Literal[True]
    else:
        reveal_type(x.flag)  # revealed: Literal[False]
```

## Negation

`not getattr(...)` correctly inverts the narrowing:

```py
class C:
    flag: bool

def _(x: C):
    if not getattr(x, "flag"):
        reveal_type(x.flag)  # revealed: Literal[False]
    else:
        reveal_type(x.flag)  # revealed: Literal[True]
```

## Multiple getattr calls with `and`

Multiple `getattr` calls in a boolean expression narrow independently:

```py
class C:
    a: bool
    b: bool

def _(x: C):
    if getattr(x, "a") and getattr(x, "b"):
        reveal_type(x.a)  # revealed: Literal[True]
        reveal_type(x.b)  # revealed: Literal[True]
```

## Aliased getattr

Narrowing works even when `getattr` is aliased, because places are created when the attribute is
accessed (e.g., `x.flag`) and narrowing uses type information to identify the `getattr` function:

```py
class C:
    flag: bool

def _(x: C):
    ga = getattr
    if ga(x, "flag"):
        reveal_type(x.flag)  # revealed: Literal[True]
    else:
        reveal_type(x.flag)  # revealed: Literal[False]
```

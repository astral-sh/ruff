# Narrowing for `isinstance` checks

Narrowing for `isinstance(object, classinfo)` expressions.

## `classinfo` is a single type

```py
def _(flag: bool):
    x = 1 if flag else "a"

    if isinstance(x, int):
        reveal_type(x)  # revealed: Literal[1]

    if isinstance(x, str):
        reveal_type(x)  # revealed: Literal["a"]
        if isinstance(x, int):
            reveal_type(x)  # revealed: Never

    if isinstance(x, (int, object)):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## `classinfo` is a tuple of types

Note: `isinstance(x, (int, str))` should not be confused with `isinstance(x, tuple[(int, str)])`.
The former is equivalent to `isinstance(x, int | str)`:

```py
def _(flag: bool, flag1: bool, flag2: bool):
    x = 1 if flag else "a"

    if isinstance(x, (int, str)):
        reveal_type(x)  # revealed: Literal[1, "a"]
    else:
        reveal_type(x)  # revealed: Never

    if isinstance(x, (int, bytes)):
        reveal_type(x)  # revealed: Literal[1]

    if isinstance(x, (bytes, str)):
        reveal_type(x)  # revealed: Literal["a"]

    # No narrowing should occur if a larger type is also
    # one of the possibilities:
    if isinstance(x, (int, object)):
        reveal_type(x)  # revealed: Literal[1, "a"]
    else:
        reveal_type(x)  # revealed: Never

    y = 1 if flag1 else "a" if flag2 else b"b"
    if isinstance(y, (int, str)):
        reveal_type(y)  # revealed: Literal[1, "a"]

    if isinstance(y, (int, bytes)):
        reveal_type(y)  # revealed: Literal[1, b"b"]

    if isinstance(y, (str, bytes)):
        reveal_type(y)  # revealed: Literal["a", b"b"]
```

## `classinfo` is a nested tuple of types

```py
def _(flag: bool):
    x = 1 if flag else "a"

    if isinstance(x, (bool, (bytes, int))):
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: Literal["a"]
```

## Class types

```py
class A: ...
class B: ...
class C: ...

x = object()

if isinstance(x, A):
    reveal_type(x)  # revealed: A
    if isinstance(x, B):
        reveal_type(x)  # revealed: A & B
    else:
        reveal_type(x)  # revealed: A & ~B

if isinstance(x, (A, B)):
    reveal_type(x)  # revealed: A | B
elif isinstance(x, (A, C)):
    reveal_type(x)  # revealed: C & ~A & ~B
else:
    # TODO: Should be simplified to ~A & ~B & ~C
    reveal_type(x)  # revealed: object & ~A & ~B & ~C
```

## No narrowing for instances of `builtins.type`

```py
def _(flag: bool):
    t = type("t", (), {})

    # This isn't testing what we want it to test if we infer anything more precise here:
    reveal_type(t)  # revealed: type

    x = 1 if flag else "foo"

    if isinstance(x, t):
        reveal_type(x)  # revealed: Literal[1, "foo"]
```

## Do not use custom `isinstance` for narrowing

```py
def _(flag: bool):
    def isinstance(x, t):
        return True
    x = 1 if flag else "a"

    if isinstance(x, int):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## Do support narrowing if `isinstance` is aliased

```py
def _(flag: bool):
    isinstance_alias = isinstance

    x = 1 if flag else "a"

    if isinstance_alias(x, int):
        reveal_type(x)  # revealed: Literal[1]
```

## Do support narrowing if `isinstance` is imported

```py
from builtins import isinstance as imported_isinstance

def _(flag: bool):
    x = 1 if flag else "a"

    if imported_isinstance(x, int):
        reveal_type(x)  # revealed: Literal[1]
```

## Do not narrow if second argument is not a type

```py
def _(flag: bool):
    x = 1 if flag else "a"

    # TODO: this should cause us to emit a diagnostic during
    # type checking
    if isinstance(x, "a"):
        reveal_type(x)  # revealed: Literal[1, "a"]

    # TODO: this should cause us to emit a diagnostic during
    # type checking
    if isinstance(x, "int"):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## Do not narrow if there are keyword arguments

```py
def _(flag: bool):
    x = 1 if flag else "a"

    # error: [unknown-argument]
    if isinstance(x, int, foo="bar"):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## `type[]` types are narrowed as well as class-literal types

```py
def _(x: object, y: type[int]):
    if isinstance(x, y):
        reveal_type(x)  # revealed: int
```

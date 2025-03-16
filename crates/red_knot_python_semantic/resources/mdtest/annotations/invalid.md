# Tests for invalid types in type expressions

## Invalid types are rejected

Many types are illegal in the context of a type expression:

```py
import typing
from knot_extensions import AlwaysTruthy, AlwaysFalsy
from typing_extensions import Literal, Never

def _(
    a: type[int],
    b: AlwaysTruthy,
    c: AlwaysFalsy,
    d: Literal[True],
    e: Literal["bar"],
    f: Literal[b"foo"],
    g: tuple[int, str],
    h: Never,
):
    def foo(): ...
    def invalid(
        i: a,  # error: [invalid-type-form] "Variable of type `type[int]` is not allowed in a type expression"
        j: b,  # error: [invalid-type-form]
        k: c,  # error: [invalid-type-form]
        l: d,  # error: [invalid-type-form]
        m: e,  # error: [invalid-type-form]
        n: f,  # error: [invalid-type-form]
        o: g,  # error: [invalid-type-form]
        p: h,  # error: [invalid-type-form]
        q: typing,  # error: [invalid-type-form]
        r: foo,  # error: [invalid-type-form]
    ):
        reveal_type(i)  # revealed: Unknown
        reveal_type(j)  # revealed: Unknown
        reveal_type(k)  # revealed: Unknown
        reveal_type(l)  # revealed: Unknown
        reveal_type(m)  # revealed: Unknown
        reveal_type(n)  # revealed: Unknown
        reveal_type(o)  # revealed: Unknown
        reveal_type(p)  # revealed: Unknown
        reveal_type(q)  # revealed: Unknown
        reveal_type(r)  # revealed: Unknown
```

## Number Literal

```py
# error: [invalid-type-form] "Number Literal is not allowed in type expressions"
def _(x: 1):
    reveal_type(x)  # revealed: Unknown
```

```py
# error: [invalid-type-form] "Number Literal is not allowed in type expressions"
def _(x: 1.1):
    reveal_type(x)  # revealed: Unknown
```

## Bytes Literal

```py
# error: [invalid-type-form] "Bytes Literal is not allowed in type expressions"
def _(x: int | b"a"):
    reveal_type(x)  # revealed: int | Unknown
```

## Boolean Literal

```py
# error: [invalid-type-form] "Boolean Literal is not allowed in type expressions"
def _(x: True):
    reveal_type(x)  # revealed: Unknown
```

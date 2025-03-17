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

## Invalid AST nodes

```py
def _(
    a: 1,  # error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
    b: 2.3,  # error: [invalid-type-form] "Float literals are not allowed in type expressions"
    c: 4j,  # error: [invalid-type-form] "Complex literals are not allowed in type expressions"
    d: True,  # error: [invalid-type-form] "Boolean literals are not allowed in this context in a type expression"
    # error: [invalid-type-form] "Bytes literals are not allowed in this context in a type expression"
    e: int | b"foo",
    f: 1 and 2,  # error: [invalid-type-form] "Boolean operations like `and` and `or` are not allowed in type expressions"
    g: 1 or 2,  # error: [invalid-type-form] "Boolean operations like `and` and `or` are not allowed in type expressions"
    h: (foo := 1),  # error: [invalid-type-form] "Named expressions like `foo := 1` are not allowed in type expressions"
    i: not 1,  # error: [invalid-type-form] "Unary operations like `not` are not allowed in type expressions"
):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: Unknown
    reveal_type(e)  # revealed: int | Unknown
    reveal_type(f)  # revealed: Unknown
    reveal_type(g)  # revealed: Unknown
    reveal_type(h)  # revealed: Unknown
    reveal_type(i)  # revealed: Unknown
```

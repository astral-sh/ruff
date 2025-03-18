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
def bar() -> None:
    return None

def _(
    a: 1,  # error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
    b: 2.3,  # error: [invalid-type-form] "Float literals are not allowed in type expressions"
    c: 4j,  # error: [invalid-type-form] "Complex literals are not allowed in type expressions"
    d: True,  # error: [invalid-type-form] "Boolean literals are not allowed in this context in a type expression"
    e: int | b"foo",  # error: [invalid-type-form] "Bytes literals are not allowed in this context in a type expression"
    f: 1 and 2,  # error: [invalid-type-form] "Boolean operations are not allowed in type expressions"
    g: 1 or 2,  # error: [invalid-type-form] "Boolean operations are not allowed in type expressions"
    h: (foo := 1),  # error: [invalid-type-form] "Named expressions are not allowed in type expressions"
    i: not 1,  # error: [invalid-type-form] "Unary operations are not allowed in type expressions"
    j: lambda: 1,  # error: [invalid-type-form] "`lambda` expressions are not allowed in type expressions"
    k: 1 if True else 2,  # error: [invalid-type-form] "`if` expressions are not allowed in type expressions"
    l: await 1,  # error: [invalid-type-form] "`await` expressions are not allowed in type expressions"
    m: (yield 1),  # error: [invalid-type-form] "`yield` expressions are not allowed in type expressions"
    n: (yield from [1]),  # error: [invalid-type-form] "`yield from` expressions are not allowed in type expressions"
    o: 1 < 2,  # error: [invalid-type-form] "Comparison expressions are not allowed in type expressions"
    p: bar(),  # error: [invalid-type-form] "Function calls are not allowed in type expressions"
    q: int | f"foo",  # error: [invalid-type-form] "F-strings are not allowed in type expressions"
    r: [1, 2, 3][1:2],  # error: [invalid-type-form] "Slices are not allowed in type expressions"
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
    reveal_type(j)  # revealed: Unknown
    reveal_type(k)  # revealed: Unknown
    reveal_type(p)  # revealed: Unknown
    reveal_type(q)  # revealed: int | Unknown
    reveal_type(r)  # revealed: @Todo(generics)
```

## Invalid Collection based AST nodes

```py
def _(
    a: {1: 2},  # error: [invalid-type-form] "Dict literals are not allowed in type expressions"
    b: {1, 2},  # error: [invalid-type-form] "Set literals are not allowed in type expressions"
    c: {k: v for k, v in [(1, 2)]},  # error: [invalid-type-form] "Dict comprehensions are not allowed in type expressions"
    d: [k for k in [1, 2]],  # error: [invalid-type-form] "List comprehensions are not allowed in type expressions"
    e: {k for k in [1, 2]},  # error: [invalid-type-form] "Set comprehensions are not allowed in type expressions"
    f: (k for k in [1, 2]),  # error: [invalid-type-form] "Generator expressions are not allowed in type expressions"
):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: Unknown
    reveal_type(e)  # revealed: Unknown
    reveal_type(f)  # revealed: Unknown
```

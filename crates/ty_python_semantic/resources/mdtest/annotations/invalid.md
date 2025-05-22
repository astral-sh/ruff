# Tests for invalid types in type expressions

## Invalid types are rejected

Many types are illegal in the context of a type expression:

```py
import typing
from ty_extensions import AlwaysTruthy, AlwaysFalsy
from typing_extensions import Literal, Never

class A: ...

def _(
    a: type[int],
    b: AlwaysTruthy,
    c: AlwaysFalsy,
    d: Literal[True],
    e: Literal["bar"],
    f: Literal[b"foo"],
    g: tuple[int, str],
    h: Never,
    i: int,
    j: A,
):
    def foo(): ...
    def invalid(
        a_: a,  # error: [invalid-type-form] "Variable of type `type[int]` is not allowed in a type expression"
        b_: b,  # error: [invalid-type-form]
        c_: c,  # error: [invalid-type-form]
        d_: d,  # error: [invalid-type-form]
        e_: e,  # error: [invalid-type-form]
        f_: f,  # error: [invalid-type-form]
        g_: g,  # error: [invalid-type-form]
        h_: h,  # error: [invalid-type-form]
        i_: typing,  # error: [invalid-type-form]
        j_: foo,  # error: [invalid-type-form]
        k_: i,  # error: [invalid-type-form] "Variable of type `int` is not allowed in a type expression"
        l_: j,  # error: [invalid-type-form] "Variable of type `A` is not allowed in a type expression"
    ):
        reveal_type(a_)  # revealed: Unknown
        reveal_type(b_)  # revealed: Unknown
        reveal_type(c_)  # revealed: Unknown
        reveal_type(d_)  # revealed: Unknown
        reveal_type(e_)  # revealed: Unknown
        reveal_type(f_)  # revealed: Unknown
        reveal_type(g_)  # revealed: Unknown
        reveal_type(h_)  # revealed: Unknown
        reveal_type(i_)  # revealed: Unknown
        reveal_type(j_)  # revealed: Unknown
```

## Invalid AST nodes

```py
def bar() -> None:
    return None

async def outer():  # avoid unrelated syntax errors on yield, yield from, and await
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
        reveal_type(r)  # revealed: @Todo(unknown type subscript)
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
    g: [int, str],  # error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: Unknown
    reveal_type(e)  # revealed: Unknown
    reveal_type(f)  # revealed: Unknown
    reveal_type(g)  # revealed: Unknown
```

## Diagnostics for common errors

<!-- snapshot-diagnostics -->

### Module-literal used when you meant to use a class from that module

It's pretty common in Python to accidentally use a module-literal type in a type expression when you
*meant* to use a class by the same name that comes from that module. We emit a nice subdiagnostic
for this case:

`foo.py`:

```py
import datetime

def f(x: datetime): ...  # error: [invalid-type-form]
```

`PIL/Image.py`:

```py
class Image: ...
```

`bar.py`:

```py
from PIL import Image

def g(x: Image): ...  # error: [invalid-type-form]
```

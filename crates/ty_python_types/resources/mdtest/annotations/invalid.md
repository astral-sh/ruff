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

# Inspired by the conformance test suite at
# https://github.com/python/typing/blob/d4f39b27a4a47aac8b6d4019e1b0b5b3156fabdc/conformance/tests/aliases_implicit.py#L88-L122
B = [x for x in range(42)]
C = {x for x in range(42)}
D = {x: y for x, y in enumerate(range(42))}
E = (x for x in range(42))

def _(
    b: B,  # error: [invalid-type-form]
    c: C,  # error: [invalid-type-form]
    d: D,  # error: [invalid-type-form]
    e: E,  # error: [invalid-type-form]
):
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: Unknown
    reveal_type(e)  # revealed: Unknown
```

## Invalid AST nodes

```py
def bar() -> None:
    return None

def outer_sync():  # `yield` from is only valid syntax inside a synchronous function
    def _(
        a: (yield from [1]),  # error: [invalid-type-form] "`yield from` expressions are not allowed in type expressions"
    ): ...

async def baz(): ...
async def outer_async():  # avoid unrelated syntax errors on `yield` and `await`
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
        l: await baz(),  # error: [invalid-type-form] "`await` expressions are not allowed in type expressions"
        m: (yield 1),  # error: [invalid-type-form] "`yield` expressions are not allowed in type expressions"
        n: 1 < 2,  # error: [invalid-type-form] "Comparison expressions are not allowed in type expressions"
        o: bar(),  # error: [invalid-type-form] "Function calls are not allowed in type expressions"
        p: int | f"foo",  # error: [invalid-type-form] "F-strings are not allowed in type expressions"
        # error: [invalid-type-form] "Slices are not allowed in type expressions"
        # error: [invalid-type-form] "Invalid subscript"
        q: [1, 2, 3][1:2],
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
        reveal_type(l)  # revealed: Unknown
        reveal_type(m)  # revealed: Unknown
        reveal_type(n)  # revealed: Unknown
        reveal_type(o)  # revealed: Unknown
        reveal_type(p)  # revealed: int | Unknown
        reveal_type(q)  # revealed: Unknown

class Mat:
    def __init__(self, value: int):
        self.value = value

    def __matmul__(self, other) -> int:
        return 42

def invalid_binary_operators(
    a: "1" + "2",  # error: [invalid-type-form] "Invalid binary operator `+` in type annotation"
    b: 3 - 5.0,  # error: [invalid-type-form] "Invalid binary operator `-` in type annotation"
    c: 4 * -2,  # error: [invalid-type-form] "Invalid binary operator `*` in type annotation"
    d: Mat(4) @ Mat(2),  # error: [invalid-type-form] "Invalid binary operator `@` in type annotation"
    e: 10 / 2,  # error: [invalid-type-form] "Invalid binary operator `/` in type annotation"
    f: 10 % 3,  # error: [invalid-type-form] "Invalid binary operator `%` in type annotation"
    g: 2**-0.5,  # error: [invalid-type-form] "Invalid binary operator `**` in type annotation"
    h: 10 // 3,  # error: [invalid-type-form] "Invalid binary operator `//` in type annotation"
    i: 1 << 2,  # error: [invalid-type-form] "Invalid binary operator `<<` in type annotation"
    j: 4 >> 42,  # error: [invalid-type-form] "Invalid binary operator `>>` in type annotation"
    k: 5 ^ 3,  # error: [invalid-type-form] "Invalid binary operator `^` in type annotation"
    l: 5 & 3,  # error: [invalid-type-form] "Invalid binary operator `&` in type annotation"
):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: Unknown
    reveal_type(e)  # revealed: Unknown
    reveal_type(f)  # revealed: Unknown
    reveal_type(g)  # revealed: Unknown
    reveal_type(h)  # revealed: Unknown
    reveal_type(i)  # revealed: Unknown
    reveal_type(j)  # revealed: Unknown
    reveal_type(k)  # revealed: Unknown
    reveal_type(l)  # revealed: Unknown
```

## Invalid Collection based AST nodes

```toml
[environment]
python-version = "3.12"
```

```py
def _(
    a: {1: 2},  # error: [invalid-type-form] "Dict literals are not allowed in type expressions"
    b: {1, 2},  # error: [invalid-type-form] "Set literals are not allowed in type expressions"
    c: {k: v for k, v in [(1, 2)]},  # error: [invalid-type-form] "Dict comprehensions are not allowed in type expressions"
    d: [k for k in [1, 2]],  # error: [invalid-type-form] "List comprehensions are not allowed in type expressions"
    e: {k for k in [1, 2]},  # error: [invalid-type-form] "Set comprehensions are not allowed in type expressions"
    f: (k for k in [1, 2]),  # error: [invalid-type-form] "Generator expressions are not allowed in type expressions"
    # error: [invalid-type-form] "List literals are not allowed in this context in a type expression: Did you mean `tuple[int, str]`?"
    g: [int, str],
    # error: [invalid-type-form] "Tuple literals are not allowed in this context in a type expression: Did you mean `tuple[int, str]`?"
    h: (int, str),
    i: (),  # error: [invalid-type-form] "Tuple literals are not allowed in this context in a type expression: Did you mean `tuple[()]`?"
):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: Unknown
    reveal_type(e)  # revealed: Unknown
    reveal_type(f)  # revealed: Unknown
    reveal_type(g)  # revealed: Unknown
    reveal_type(h)  # revealed: Unknown
    reveal_type(i)  # revealed: Unknown

# error: [invalid-type-form] "List literals are not allowed in this context in a type expression: Did you mean `list[int]`?"
class name_0[name_2: [int]]:
    pass

# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
# error: [invalid-type-form] "Dict literals are not allowed in type expressions"
class name_4[name_1: [{}]]:
    pass
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

### List-literal used when you meant to use a list or tuple

```py
def _(
    x: [int],  # error: [invalid-type-form]
) -> [int]:  # error: [invalid-type-form]
    return x
```

```py
def _(
    x: [int, str],  # error: [invalid-type-form]
) -> [int, str]:  # error: [invalid-type-form]
    return x
```

### Tuple-literal used when you meant to use a tuple

```py
def _(
    x: (),  # error: [invalid-type-form]
) -> ():  # error: [invalid-type-form]
    return x
```

```py
def _(
    x: (int,),  # error: [invalid-type-form]
) -> (int,):  # error: [invalid-type-form]
    return x
```

```py
def _(
    x: (int, str),  # error: [invalid-type-form]
) -> (int, str):  # error: [invalid-type-form]
    return x
```

### Special-cased diagnostic for `callable` used in a type expression

```py
# error: [invalid-type-form]
# error: [invalid-type-form]
def decorator(fn: callable) -> callable:
    return fn
```

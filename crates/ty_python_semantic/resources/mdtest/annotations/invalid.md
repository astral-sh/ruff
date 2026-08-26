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
        a_: a,  # error: [invalid-type-form] "Variable of type `type[int]` is not allowed in a parameter annotation"
        b_: b,  # error: [invalid-type-form]
        c_: c,  # error: [invalid-type-form]
        d_: d,  # error: [invalid-type-form]
        e_: e,  # error: [invalid-type-form]
        f_: f,  # error: [invalid-type-form]
        g_: g,  # error: [invalid-type-form]
        h_: h,  # error: [invalid-type-form]
        i_: typing,  # error: [invalid-type-form]
        j_: foo,  # error: [invalid-type-form]
        k_: i,  # error: [invalid-type-form] "Variable of type `int` is not allowed in a parameter annotation"
        l_: j,  # error: [invalid-type-form] "Variable of type `A` is not allowed in a parameter annotation"
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
from typing import TypeVar

T = TypeVar("T")

def bar() -> None:
    return None

def outer_sync():  # `yield` from is only valid syntax inside a synchronous function
    def _(
        a: (yield from [1]),  # error: [invalid-type-form] "`yield from` expressions are not allowed in parameter annotations"
    ): ...

async def baz(): ...
async def outer_async():  # avoid unrelated syntax errors on `yield` and `await`
    def _(
        a: 1,  # error: [invalid-type-form] "Int literals are not allowed in this context in a parameter annotation"
        b: 2.3,  # error: [invalid-type-form] "Float literals are not allowed in parameter annotations"
        c: 4j,  # error: [invalid-type-form] "Complex literals are not allowed in parameter annotations"
        d: True,  # error: [invalid-type-form] "Boolean literals are not allowed in this context in a parameter annotation"
        # error: [unsupported-operator]
        # error: [invalid-type-form] "Bytes literals are not allowed in this context in a parameter annotation"
        e: int | b"foo",
        f: 1 and 2,  # error: [invalid-type-form] "Boolean operations are not allowed in parameter annotations"
        g: 1 or 2,  # error: [invalid-type-form] "Boolean operations are not allowed in parameter annotations"
        h: (foo := 1),  # error: [invalid-type-form] "Named expressions are not allowed in parameter annotations"
        i: not 1,  # error: [invalid-type-form] "Unary operations are not allowed in parameter annotations"
        j: lambda: 1,  # error: [invalid-type-form] "`lambda` expressions are not allowed in parameter annotations"
        k: 1 if True else 2,  # error: [invalid-type-form] "`if` expressions are not allowed in parameter annotations"
        l: await baz(),  # error: [invalid-type-form] "`await` expressions are not allowed in parameter annotations"
        m: (yield 1),  # error: [invalid-type-form] "`yield` expressions are not allowed in parameter annotations"
        n: 1 < 2,  # error: [invalid-type-form] "Comparison expressions are not allowed in parameter annotations"
        o: bar(),  # error: [invalid-type-form] "Function calls are not allowed in parameter annotations"
        # error: [unsupported-operator]
        # error: [invalid-type-form] "F-strings are not allowed in parameter annotations"
        p: int | f"foo",
        # error: [invalid-type-form] "Only simple names and dotted names can be subscripted in parameter annotations"
        q: [1, 2, 3][1:2],
        # error: [invalid-type-form] "Only simple names and dotted names can be subscripted in parameter annotations"
        r: list[T][int],
        # error: [invalid-type-form] "Only simple names and dotted names can be subscripted in parameter annotations"
        s: list[list[T][int]],
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
    # error: [invalid-type-form] "Int literals are not allowed in this context in a parameter annotation"
    # error: [invalid-type-form] "Int literals are not allowed in this context in a parameter annotation"
    l: 5 & 3,
    # error: [invalid-type-form] "Int literals are not allowed in this context in a parameter annotation"
    m: ~3,
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
    reveal_type(m)  # revealed: Unknown
```

## Error recovery upon encountering invalid AST nodes

Upon encountering an invalid-in-type-expression AST node, we try to avoid cascading diagnostics. For
example, in this snippet, we only report the the outer list literal is invalid, and ignore the fact
that there is also an invalid list literal inside the outer list literal node:

```py
# error: [invalid-type-form]
x: [[int]]
```

However, runtime errors inside invalid AST nodes are still reported -- these errors are more serious
than just "typing spec pedantry":

```py
# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
# error: [unresolved-reference] "Name `foo` used when not defined"
x: [[foo]]
```

But we avoid false-positive diagnostics regarding unresolved references inside string annotations if
we detect that the string annotation is an invalid type form. These diagnostics would just add
noise, since stringized annotations are never executed at runtime. The following snippet causes us
to emit `invalid-type-form`, but we ignore that `foo` is an "unresolved reference" inside the string
annotation:

```py
# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
x: "[[foo]]"
```

## Invalid subscript operands in string annotations

Invalid subscript operands in string annotations must not be evaluated. In particular, lambda
defaults and functional `TypedDict` arguments should not produce cascading unresolved-reference
diagnostics, and assignment expressions should not cause a panic.

`runtime.py`:

```py
from typing_extensions import TypedDict

# error: [invalid-type-form] "Only simple names and dotted names can be subscripted in type expressions"
a: "(lambda value=missing: None)[int]"
# error: [invalid-type-form] "Only simple names and dotted names can be subscripted in type expressions"
b: "(lambda value=(name := int): None)[int]"
# error: [invalid-type-form] "Only simple names and dotted names can be subscripted in type expressions"
c: "TypedDict('T', {}, extra_items=missing)[int]"
```

The same error-recovery behavior applies to annotations in stub files:

`stub.pyi`:

```pyi
from typing_extensions import TypedDict

# error: [invalid-type-form] "Only simple names and dotted names can be subscripted in type expressions"
a: "(lambda value=missing: None)[int]"
# error: [invalid-type-form] "Only simple names and dotted names can be subscripted in type expressions"
b: "(lambda value=(name := int): None)[int]"
# error: [invalid-type-form] "Only simple names and dotted names can be subscripted in type expressions"
c: "TypedDict('T', {}, extra_items=missing)[int]"
```

## Invalid subscript arguments in string annotations

Even when a subscript's operand is a valid name, it might not be a generic type. We reject such
specializations without evaluating their arguments in string annotations. Unsupported `type[...]`
arguments are checked as type expressions while retaining their existing fallback type.

`runtime.py`:

```py
from typing import Any, Tuple

# error: [invalid-type-form] "Non-generic class `int` cannot be specialized"
a: "int[(name := missing)]"
# error: [invalid-type-form] "Non-generic class `int` cannot be specialized"
b: "type[int[(name := missing)]]"
# error: [invalid-type-form] "Named expressions are not allowed"
c: "type[(name := missing)]"
# error: [invalid-type-form] "Named expressions are not allowed"
d: "type[Any[(name := missing)]]"
# error: [invalid-type-form] "`lambda` expressions are not allowed"
e: "type[Tuple[lambda default=(name := missing): None]]"
# error: [invalid-type-form] "`lambda` expressions are not allowed"
f: "type[lambda default=(name := missing): None]"
```

Stub files use the same error recovery.

`stub.pyi`:

```pyi
from typing import Any, Tuple

# error: [invalid-type-form] "Non-generic class `int` cannot be specialized"
a: "int[(name := missing)]"
# error: [invalid-type-form] "Non-generic class `int` cannot be specialized"
b: "type[int[(name := missing)]]"
# error: [invalid-type-form] "Named expressions are not allowed"
c: "type[(name := missing)]"
# error: [invalid-type-form] "Named expressions are not allowed"
d: "type[Any[(name := missing)]]"
# error: [invalid-type-form] "`lambda` expressions are not allowed"
e: "type[Tuple[lambda default=(name := missing): None]]"
# error: [invalid-type-form] "`lambda` expressions are not allowed"
f: "type[lambda default=(name := missing): None]"
```

## Invalid subscript arguments in evaluated annotations

For annotations that are evaluated, we report invalid type arguments and errors encountered while
evaluating them.

```toml
[environment]
python-version = "3.13"
```

```py
# error: [invalid-type-form] "Non-generic class `int` cannot be specialized"
# error: [unresolved-reference] "Name `missing` used when not defined"
a: int[(name := missing)]

# error: [invalid-type-form] "Named expressions are not allowed"
# error: [unresolved-reference] "Name `other_missing` used when not defined"
b: type[(other := other_missing)]

# error: [invalid-type-form] "Function calls are not allowed"
# error: [unresolved-reference] "Name `missing_call` used when not defined"
c: type[missing_call()]
```

## Multiple starred expressions in a `tuple` specialization

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.11"
```

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

def f(
    # error: [invalid-type-form] "Multiple unpacked variadic tuples are not allowed in a `tuple` specialization"
    x: tuple[*tuple[int, ...], *tuple[str, ...]],
    # error: [invalid-type-form] "Multiple unpacked variadic tuples are not allowed in a `tuple` specialization"
    x2: tuple[Unpack[tuple[int, ...]], Unpack[tuple[str, ...]]],
    y: tuple[*tuple[int, ...], str, int, *tuple[str, ...]],  # error: [invalid-type-form]
    y2: tuple[Unpack[tuple[int, ...]], str, int, Unpack[tuple[str, ...]]],  # error: [invalid-type-form]
    # Multiple unpacked elements are fine, as long as the unpacked elements are not variadic:
    z: tuple[*tuple[int, ...], *tuple[str]],
    z2: tuple[Unpack[tuple[int, ...]], Unpack[tuple[str]]],
):
    reveal_type(x)  # revealed: tuple[int | str, ...]
    reveal_type(x2)  # revealed: tuple[int | str, ...]
    reveal_type(y)  # revealed: tuple[str | int, ...]
    reveal_type(y2)  # revealed: tuple[str | int, ...]
    reveal_type(z)  # revealed: tuple[*tuple[int, ...], str]
    reveal_type(z2)  # revealed: tuple[*tuple[int, ...], str]

T1 = tuple[int, *Ts, str, *Ts]  # error: [invalid-type-form]

def func3(t: tuple[*Ts]):
    t5: tuple[*tuple[str], *Ts]  # OK
    t6: tuple[*tuple[str, ...], *Ts]  # error: [invalid-type-form]
```

## Ellipses in the wrong place in a `tuple` specialization

```toml
[environment]
python-version = "3.11"
```

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

t1: tuple[int, ...]
# error: [invalid-type-form] "Invalid `tuple` specialization: `...` can only be used as the second element in a two-element `tuple` specialization"
t2: tuple[int, int, ...]
# error: [invalid-type-form] "Invalid `tuple` specialization: `...` can only be used as the second element in a two-element `tuple` specialization"
t3: tuple[...]
# error: [invalid-type-form] "Invalid `tuple` specialization: `...` can only be used as the second element in a two-element `tuple` specialization"
t4: tuple[..., int]
# error: [invalid-type-form] "Invalid `tuple` specialization: `...` can only be used as the second element in a two-element `tuple` specialization"
t5: tuple[int, ..., int]
# error: [invalid-type-form] "Invalid `tuple` specialization: `...` cannot be used after an unpacked element"
t6: tuple[*tuple[str], ...]
# error: [invalid-type-form] "Invalid `tuple` specialization: `...` cannot be used after an unpacked element"
t7: tuple[*tuple[str, ...], ...]

def invalid_typevartuple_ellipsis(
    # error: [invalid-type-form] "Invalid `tuple` specialization: `...` cannot be used after an unpacked element"
    starred: tuple[*Ts, ...],
    # error: [invalid-type-form] "Invalid `tuple` specialization: `...` cannot be used after an unpacked element"
    unpacked: tuple[Unpack[Ts], ...],
) -> None: ...
```

## Invalid AST nodes in string annotations

Invalid AST nodes should also be rejected when they appear in string annotations:

```py
def bar() -> None:
    return None

async def baz(): ...
async def outer_async():  # avoid unrelated syntax errors on `yield` and `await`
    def _(
        a: "1",  # error: [invalid-type-form] "Int literals are not allowed in this context in a parameter annotation"
        b: "2.3",  # error: [invalid-type-form] "Float literals are not allowed in parameter annotations"
        c: "4j",  # error: [invalid-type-form] "Complex literals are not allowed in parameter annotations"
        d: "True",  # error: [invalid-type-form] "Boolean literals are not allowed in this context in a parameter annotation"
        e: "1 and 2",  # error: [invalid-type-form] "Boolean operations are not allowed in parameter annotations"
        f: "1 or 2",  # error: [invalid-type-form] "Boolean operations are not allowed in parameter annotations"
        g: "(foo := 1)",  # error: [invalid-type-form] "Named expressions are not allowed in parameter annotations"
        h: "not 1",  # error: [invalid-type-form] "Unary operations are not allowed in parameter annotations"
        i: "lambda: 1",  # error: [invalid-type-form] "`lambda` expressions are not allowed in parameter annotations"
        j: "1 if True else 2",  # error: [invalid-type-form] "`if` expressions are not allowed in parameter annotations"
        k: "await baz()",  # error: [invalid-type-form] "`await` expressions are not allowed in parameter annotations"
        l: "(yield 1)",  # error: [invalid-type-form] "`yield` expressions are not allowed in parameter annotations"
        m: "1 < 2",  # error: [invalid-type-form] "Comparison expressions are not allowed in parameter annotations"
        n: "bar()",  # error: [invalid-type-form] "Function calls are not allowed in parameter annotations"
        # error: [invalid-type-form] "Only simple names and dotted names can be subscripted in parameter annotations"
        o: "[1, 2, 3][1:2]",
        # error: [invalid-type-form] "Only simple names, dotted names and subscripts can be used in parameter annotations"
        p: list[int].append,
        # error: [invalid-type-form] "Only simple names, dotted names and subscripts can be used in parameter annotations"
        q: list[list[int].append],
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
        reveal_type(m)  # revealed: Unknown
        reveal_type(n)  # revealed: Unknown
        reveal_type(o)  # revealed: Unknown
        reveal_type(p)  # revealed: Unknown
        reveal_type(q)  # revealed: list[Unknown]
```

## Invalid Collection based AST nodes

```toml
[environment]
python-version = "3.12"
```

```py
def _(
    a: {1: 2},  # error: [invalid-type-form] "Dict literals are not allowed in parameter annotations"
    b: {1, 2},  # error: [invalid-type-form] "Set literals are not allowed in parameter annotations"
    c: {k: v for k, v in [(1, 2)]},  # error: [invalid-type-form] "Dict comprehensions are not allowed in parameter annotations"
    d: [k for k in [1, 2]],  # error: [invalid-type-form] "List comprehensions are not allowed in parameter annotations"
    e: {k for k in [1, 2]},  # error: [invalid-type-form] "Set comprehensions are not allowed in parameter annotations"
    f: (k for k in [1, 2]),  # error: [invalid-type-form] "Generator expressions are not allowed in parameter annotations"
    # error: [invalid-type-form] "List literals are not allowed in this context in a parameter annotation"
    g: [int, str],
    # error: [invalid-type-form] "Tuple literals are not allowed in this context in a parameter annotation: Did you mean `tuple[int, str]`?"
    h: (int, str),
    i: (),  # error: [invalid-type-form] "Tuple literals are not allowed in this context in a parameter annotation: Did you mean `tuple[()]`?"
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
class name_4[name_1: [{}]]:
    pass
```

## Diagnostics for common errors

### Module-literal used when you meant to use a class from that module

<!-- snapshot-diagnostics -->

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

### Collection literals used as type expressions

Collection literals are not valid type expressions. When the intended collection type is clear, we
suggest a subscripted builtin and offer an unsafe fix when that builtin is available.

#### List literals

A list literal with one element suggests a `list` annotation. We offer a fix in both parameter and
return annotations.

```py
def _(
    x: [int],  # snapshot: invalid-type-form
) -> [int]:  # snapshot: invalid-type-form
    return x
```

```snapshot
error[invalid-type-form]: List literals are not allowed in this context in a parameter annotation
 --> src/mdtest_snippet.py:2:8
  |
2 |     x: [int],  # snapshot: invalid-type-form
  |        ^^^^^ Did you mean `list[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `list[...]`
  |
1 | def _(
  -     x: [int],  # snapshot: invalid-type-form
2 +     x: list[int],  # snapshot: invalid-type-form
3 | ) -> [int]:  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: List literals are not allowed in this context in a return type annotation
 --> src/mdtest_snippet.py:3:6
  |
3 | ) -> [int]:  # snapshot: invalid-type-form
  |      ^^^^^ Did you mean `list[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `list[...]`
  |
2 |     x: [int],  # snapshot: invalid-type-form
  - ) -> [int]:  # snapshot: invalid-type-form
3 + ) -> list[int]:  # snapshot: invalid-type-form
4 |     return x
  |
note: This is an unsafe fix and may change runtime behavior
```

A list literal with several elements is ambiguous, so we do not suggest a replacement.

```py
def _(
    x: [int, str],  # snapshot: invalid-type-form
) -> [int, str]:  # snapshot: invalid-type-form
    return x
```

```snapshot
error[invalid-type-form]: List literals are not allowed in this context in a parameter annotation
 --> src/mdtest_snippet.py:6:8
  |
6 |     x: [int, str],  # snapshot: invalid-type-form
  |        ^^^^^^^^^^
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: List literals are not allowed in this context in a return type annotation
 --> src/mdtest_snippet.py:7:6
  |
7 | ) -> [int, str]:  # snapshot: invalid-type-form
  |      ^^^^^^^^^^
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

#### Tuple literals

An empty tuple literal suggests `tuple[()]`, the type of an empty tuple.

```py
def _(
    x: (),  # snapshot: invalid-type-form
) -> ():  # snapshot: invalid-type-form
    return x
```

```snapshot
error[invalid-type-form]: Tuple literals are not allowed in this context in a parameter annotation
 --> src/mdtest_snippet.py:2:8
  |
2 |     x: (),  # snapshot: invalid-type-form
  |        ^^ Did you mean `tuple[()]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
  |
1 | def _(
  -     x: (),  # snapshot: invalid-type-form
2 +     x: tuple[()],  # snapshot: invalid-type-form
3 | ) -> ():  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Tuple literals are not allowed in this context in a return type annotation
 --> src/mdtest_snippet.py:3:6
  |
3 | ) -> ():  # snapshot: invalid-type-form
  |      ^^ Did you mean `tuple[()]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
  |
2 |     x: (),  # snapshot: invalid-type-form
  - ) -> ():  # snapshot: invalid-type-form
3 + ) -> tuple[()]:  # snapshot: invalid-type-form
4 |     return x
  |
note: This is an unsafe fix and may change runtime behavior
```

A tuple literal with one element suggests a fixed-length tuple with one element.

```py
def _(
    x: (int,),  # snapshot: invalid-type-form
) -> (int,):  # snapshot: invalid-type-form
    return x
```

```snapshot
error[invalid-type-form]: Tuple literals are not allowed in this context in a parameter annotation
 --> src/mdtest_snippet.py:6:8
  |
6 |     x: (int,),  # snapshot: invalid-type-form
  |        ^^^^^^ Did you mean `tuple[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
  |
5 | def _(
  -     x: (int,),  # snapshot: invalid-type-form
6 +     x: tuple[int],  # snapshot: invalid-type-form
7 | ) -> (int,):  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Tuple literals are not allowed in this context in a return type annotation
 --> src/mdtest_snippet.py:7:6
  |
7 | ) -> (int,):  # snapshot: invalid-type-form
  |      ^^^^^^ Did you mean `tuple[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
  |
6 |     x: (int,),  # snapshot: invalid-type-form
  - ) -> (int,):  # snapshot: invalid-type-form
7 + ) -> tuple[int]:  # snapshot: invalid-type-form
8 |     return x
  |
note: This is an unsafe fix and may change runtime behavior
```

A tuple literal with several elements suggests a fixed-length tuple with the corresponding element
types.

```py
def _(
    x: (int, str),  # snapshot: invalid-type-form
) -> (int, str):  # snapshot: invalid-type-form
    return x
```

```snapshot
error[invalid-type-form]: Tuple literals are not allowed in this context in a parameter annotation
  --> src/mdtest_snippet.py:10:8
   |
10 |     x: (int, str),  # snapshot: invalid-type-form
   |        ^^^^^^^^^^ Did you mean `tuple[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
   |
9  | def _(
   -     x: (int, str),  # snapshot: invalid-type-form
10 +     x: tuple[int, str],  # snapshot: invalid-type-form
11 | ) -> (int, str):  # snapshot: invalid-type-form
   |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Tuple literals are not allowed in this context in a return type annotation
  --> src/mdtest_snippet.py:11:6
   |
11 | ) -> (int, str):  # snapshot: invalid-type-form
   |      ^^^^^^^^^^ Did you mean `tuple[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
   |
10 |     x: (int, str),  # snapshot: invalid-type-form
   - ) -> (int, str):  # snapshot: invalid-type-form
11 + ) -> tuple[int, str]:  # snapshot: invalid-type-form
12 |     return x
   |
note: This is an unsafe fix and may change runtime behavior
```

#### Dict and set literals

A dictionary literal with one entry suggests `dict[Key, Value]`, and a set literal with one element
suggests `set[Element]`.

```py
def _(
    x: {int: str},  # snapshot: invalid-type-form
    y: {str},  # snapshot: invalid-type-form
): ...
```

```snapshot
error[invalid-type-form]: Dict literals are not allowed in parameter annotations
 --> src/mdtest_snippet.py:2:8
  |
2 |     x: {int: str},  # snapshot: invalid-type-form
  |        ^^^^^^^^^^ Did you mean `dict[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `dict[...]`
  |
1 | def _(
  -     x: {int: str},  # snapshot: invalid-type-form
2 +     x: dict[int, str],  # snapshot: invalid-type-form
3 |     y: {str},  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Set literals are not allowed in parameter annotations
 --> src/mdtest_snippet.py:3:8
  |
3 |     y: {str},  # snapshot: invalid-type-form
  |        ^^^^^ Did you mean `set[str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `set[...]`
  |
2 |     x: {int: str},  # snapshot: invalid-type-form
  -     y: {str},  # snapshot: invalid-type-form
3 +     y: set[str],  # snapshot: invalid-type-form
4 | ): ...
  |
note: This is an unsafe fix and may change runtime behavior
```

#### Parenthesized collection elements

Rewriting a tuple literal preserves parentheses around its first and last elements, including nested
parentheses and parentheses around the entire tuple.

```py
# fmt: off
first: ((int), str)  # snapshot: invalid-type-form
last: (int, (str))  # snapshot: invalid-type-form
single: (((int)),)  # snapshot: invalid-type-form
outer: (((int), (str)))  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: Tuple literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:2:8
  |
2 | first: ((int), str)  # snapshot: invalid-type-form
  |        ^^^^^^^^^^^^ Did you mean `tuple[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
  |
1 | # fmt: off
  - first: ((int), str)  # snapshot: invalid-type-form
2 + first: tuple[(int), str]  # snapshot: invalid-type-form
3 | last: (int, (str))  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Tuple literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:3:7
  |
3 | last: (int, (str))  # snapshot: invalid-type-form
  |       ^^^^^^^^^^^^ Did you mean `tuple[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
  |
2 | first: ((int), str)  # snapshot: invalid-type-form
  - last: (int, (str))  # snapshot: invalid-type-form
3 + last: tuple[int, (str)]  # snapshot: invalid-type-form
4 | single: (((int)),)  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Tuple literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:4:9
  |
4 | single: (((int)),)  # snapshot: invalid-type-form
  |         ^^^^^^^^^^ Did you mean `tuple[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
  |
3 | last: (int, (str))  # snapshot: invalid-type-form
  - single: (((int)),)  # snapshot: invalid-type-form
4 + single: tuple[((int))]  # snapshot: invalid-type-form
5 | outer: (((int), (str)))  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Tuple literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:5:9
  |
5 | outer: (((int), (str)))  # snapshot: invalid-type-form
  |         ^^^^^^^^^^^^^^ Did you mean `tuple[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `tuple[...]`
  |
4 | single: (((int)),)  # snapshot: invalid-type-form
  - outer: (((int), (str)))  # snapshot: invalid-type-form
5 + outer: (tuple[(int), (str)])  # snapshot: invalid-type-form
6 | # fmt: off
  |
note: This is an unsafe fix and may change runtime behavior
```

Dictionary keys and values, set elements, and list elements also retain their parentheses.

```py
# fmt: off
key: {(int): str}  # snapshot: invalid-type-form
value: {int: ((str)),}  # snapshot: invalid-type-form
items: {((int)),}  # snapshot: invalid-type-form
values: [((int))]  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: Dict literals are not allowed in type expressions
 --> src/mdtest_snippet.py:7:6
  |
7 | key: {(int): str}  # snapshot: invalid-type-form
  |      ^^^^^^^^^^^^ Did you mean `dict[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `dict[...]`
  |
6 | # fmt: off
  - key: {(int): str}  # snapshot: invalid-type-form
7 + key: dict[(int), str]  # snapshot: invalid-type-form
8 | value: {int: ((str)),}  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Dict literals are not allowed in type expressions
 --> src/mdtest_snippet.py:8:8
  |
8 | value: {int: ((str)),}  # snapshot: invalid-type-form
  |        ^^^^^^^^^^^^^^^ Did you mean `dict[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `dict[...]`
  |
7 | key: {(int): str}  # snapshot: invalid-type-form
  - value: {int: ((str)),}  # snapshot: invalid-type-form
8 + value: dict[int, ((str))]  # snapshot: invalid-type-form
9 | items: {((int)),}  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Set literals are not allowed in type expressions
 --> src/mdtest_snippet.py:9:8
  |
9 | items: {((int)),}  # snapshot: invalid-type-form
  |        ^^^^^^^^^^ Did you mean `set[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `set[...]`
   |
8  | value: {int: ((str)),}  # snapshot: invalid-type-form
   - items: {((int)),}  # snapshot: invalid-type-form
9  + items: set[((int))]  # snapshot: invalid-type-form
10 | values: [((int))]  # snapshot: invalid-type-form
   |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: List literals are not allowed in this context in a type expression
  --> src/mdtest_snippet.py:10:9
   |
10 | values: [((int))]  # snapshot: invalid-type-form
   |         ^^^^^^^^^ Did you mean `list[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `list[...]`
   |
9  | items: {((int)),}  # snapshot: invalid-type-form
   - values: [((int))]  # snapshot: invalid-type-form
10 + values: list[((int))]  # snapshot: invalid-type-form
   |
note: This is an unsafe fix and may change runtime behavior
```

#### Required parentheses in collection elements

Some expressions require parentheses inside a subscript. Although `yield` expressions are not valid
type expressions, the fix preserves their parentheses to avoid introducing a syntax error.

```py
def generator():
    yielded_key: {(yield int): str}  # snapshot: invalid-type-form
    yielded_value: {int: (yield str)}  # snapshot: invalid-type-form
    yielded_element: {(yield int)}  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: Dict literals are not allowed in type expressions
 --> src/mdtest_snippet.py:2:18
  |
2 |     yielded_key: {(yield int): str}  # snapshot: invalid-type-form
  |                  ^^^^^^^^^^^^^^^^^^
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `dict[...]`
  |
1 | def generator():
  -     yielded_key: {(yield int): str}  # snapshot: invalid-type-form
2 +     yielded_key: dict[(yield int), str]  # snapshot: invalid-type-form
3 |     yielded_value: {int: (yield str)}  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Dict literals are not allowed in type expressions
 --> src/mdtest_snippet.py:3:20
  |
3 |     yielded_value: {int: (yield str)}  # snapshot: invalid-type-form
  |                    ^^^^^^^^^^^^^^^^^^
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `dict[...]`
  |
2 |     yielded_key: {(yield int): str}  # snapshot: invalid-type-form
  -     yielded_value: {int: (yield str)}  # snapshot: invalid-type-form
3 +     yielded_value: dict[int, (yield str)]  # snapshot: invalid-type-form
4 |     yielded_element: {(yield int)}  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior


error[invalid-type-form]: Set literals are not allowed in type expressions
 --> src/mdtest_snippet.py:4:22
  |
4 |     yielded_element: {(yield int)}  # snapshot: invalid-type-form
  |                      ^^^^^^^^^^^^^
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `set[...]`
  |
3 |     yielded_value: {int: (yield str)}  # snapshot: invalid-type-form
  -     yielded_element: {(yield int)}  # snapshot: invalid-type-form
4 +     yielded_element: set[(yield int)]  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior
```

#### Collection literal fixes require Python 3.9 or later

Builtin collection types cannot be subscripted on Python 3.8, so their literals do not receive fixes
that would introduce unsupported subscripts.

```toml
[environment]
python-version = "3.8"
```

```py
as_list: [int]  # snapshot: invalid-type-form
as_tuple: (int,)  # snapshot: invalid-type-form
as_dict: {str: int}  # snapshot: invalid-type-form
as_set: {int}  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: List literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:1:10
  |
1 | as_list: [int]  # snapshot: invalid-type-form
  |          ^^^^^ Did you mean `list[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: Tuple literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:2:11
  |
2 | as_tuple: (int,)  # snapshot: invalid-type-form
  |           ^^^^^^ Did you mean `tuple[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: Dict literals are not allowed in type expressions
 --> src/mdtest_snippet.py:3:10
  |
3 | as_dict: {str: int}  # snapshot: invalid-type-form
  |          ^^^^^^^^^^ Did you mean `dict[str, int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: Set literals are not allowed in type expressions
 --> src/mdtest_snippet.py:4:9
  |
4 | as_set: {int}  # snapshot: invalid-type-form
  |         ^^^^^ Did you mean `set[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

#### Collection literal fixes are omitted for starred elements

Starred subscripts are unavailable before Python 3.11, so starred collection elements cannot be
rewritten into subscripts when targeting Python 3.10.

```toml
[environment]
python-version = "3.10"
```

```py
types = (int, str)

as_list: [*types]  # snapshot: invalid-type-form
as_tuple: (*types,)  # snapshot: invalid-type-form
as_set: {*types}  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: List literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:3:10
  |
3 | as_list: [*types]  # snapshot: invalid-type-form
  |          ^^^^^^^^ Did you mean `list[tuple[Unknown, ...]]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: Tuple literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:4:11
  |
4 | as_tuple: (*types,)  # snapshot: invalid-type-form
  |           ^^^^^^^^^ Did you mean `tuple[tuple[Unknown, ...]]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: Set literals are not allowed in type expressions
 --> src/mdtest_snippet.py:5:9
  |
5 | as_set: {*types}  # snapshot: invalid-type-form
  |         ^^^^^^^^ Did you mean `set[tuple[Unknown, ...]]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

#### Collection literal fixes are omitted for multiline annotations

Multiline collection literals can contain comments that would be removed by replacing their
delimiters, so we do not offer an autofix.

`list.py`:

```py
values: [  # snapshot: invalid-type-form
    # The element must not be discarded.
    int,
]
```

```snapshot
error[invalid-type-form]: List literals are not allowed in this context in a type expression
 --> src/list.py:1:9
  |
1 |   values: [  # snapshot: invalid-type-form
  |  _________^
2 | |     # The element must not be discarded.
3 | |     int,
4 | | ]
  | |_^ Did you mean `list[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

The same restriction applies to tuple literals.

`tuple.py`:

```py
value: (  # snapshot: invalid-type-form
    # The first type remains documented.
    int,
    str,  # The final type remains documented.
)
```

```snapshot
error[invalid-type-form]: Tuple literals are not allowed in this context in a type expression
 --> src/tuple.py:1:8
  |
1 |   value: (  # snapshot: invalid-type-form
  |  ________^
2 | |     # The first type remains documented.
3 | |     int,
4 | |     str,  # The final type remains documented.
5 | | )
  | |_^ Did you mean `tuple[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

A dictionary literal may have comments around its key, colon, value, or trailing comma.

`dict.py`:

```py
mapping: {  # snapshot: invalid-type-form
    # The key remains documented.
    str:  # The separator remains documented.
    # The value remains documented.
    int,  # The trailing comma remains documented.
}
```

```snapshot
error[invalid-type-form]: Dict literals are not allowed in type expressions
 --> src/dict.py:1:10
  |
1 |   mapping: {  # snapshot: invalid-type-form
  |  __________^
2 | |     # The key remains documented.
3 | |     str:  # The separator remains documented.
4 | |     # The value remains documented.
5 | |     int,  # The trailing comma remains documented.
6 | | }
  | |_^ Did you mean `dict[str, int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

Set literals can likewise contain comments around their element.

`set.py`:

```py
items: {  # snapshot: invalid-type-form
    # The element remains documented.
    int,  # The trailing comma remains documented.
}
```

```snapshot
error[invalid-type-form]: Set literals are not allowed in type expressions
 --> src/set.py:1:8
  |
1 |   items: {  # snapshot: invalid-type-form
  |  ________^
2 | |     # The element remains documented.
3 | |     int,  # The trailing comma remains documented.
4 | | }
  | |_^ Did you mean `set[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

#### Class attributes do not shadow collection builtins in methods

A class attribute named `set` is not visible when resolving names in a method body, so it does not
prevent an annotation from being rewritten with the builtin `set`.

```py
class Container:
    set = 42

    def check(self):
        value: {int}  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: Set literals are not allowed in type expressions
 --> src/mdtest_snippet.py:5:16
  |
5 |         value: {int}  # snapshot: invalid-type-form
  |                ^^^^^ Did you mean `set[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `set[...]`
  |
4 |     def check(self):
  -         value: {int}  # snapshot: invalid-type-form
5 +         value: set[int]  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior
```

#### Class attributes in nested annotation scopes

A generic type alias can access attributes of its enclosing class through its type-parameter scope.
A class attribute named `list` therefore shadows the builtin in the alias's value.

```toml
[environment]
python-version = "3.12"
```

```py
class C:
    list = 42

    # TODO: `visible_ancestor_scopes` skips the class through nested annotation scopes,
    # so we incorrectly offer a fix that resolves `list` to `C.list`.
    type Alias[T] = [int]  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: List literals are not allowed in this context in a type alias value
 --> src/mdtest_snippet.py:6:21
  |
6 |     type Alias[T] = [int]  # snapshot: invalid-type-form
  |                     ^^^^^ Did you mean `list[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `list[...]`
  |
5 |     # so we incorrectly offer a fix that resolves `list` to `C.list`.
  -     type Alias[T] = [int]  # snapshot: invalid-type-form
6 +     type Alias[T] = list[int]  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior
```

#### Collection literal fixes with project-level builtin overrides

A project-level `__builtins__.pyi` can replace `list` while leaving the standard `set` builtin
available. We suppress only the fix that would reference the overridden builtin.

```py
overridden: [int]  # snapshot: invalid-type-form
standard: {int}  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: List literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:1:13
  |
1 | overridden: [int]  # snapshot: invalid-type-form
  |             ^^^^^ Did you mean `list[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: Set literals are not allowed in type expressions
 --> src/mdtest_snippet.py:2:11
  |
2 | standard: {int}  # snapshot: invalid-type-form
  |           ^^^^^ Did you mean `set[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
help: Replace with `set[...]`
  |
1 | overridden: [int]  # snapshot: invalid-type-form
  - standard: {int}  # snapshot: invalid-type-form
2 + standard: set[int]  # snapshot: invalid-type-form
  |
note: This is an unsafe fix and may change runtime behavior
```

`__builtins__.pyi`:

```pyi
list: object
```

#### Collection literal fixes are omitted in string annotations

Collection literals parsed from quoted annotations do not have source ranges that can be rewritten
directly, so their diagnostics do not offer collection-literal fixes.

```py
quoted_list: "[int]"  # snapshot: invalid-type-form
quoted_tuple: "(int, str)"  # snapshot: invalid-type-form
quoted_dict: "{int: str}"  # snapshot: invalid-type-form
quoted_set: "{int}"  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: List literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:1:15
  |
1 | quoted_list: "[int]"  # snapshot: invalid-type-form
  |               ^^^^^ Did you mean `list[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: Tuple literals are not allowed in this context in a type expression
 --> src/mdtest_snippet.py:2:16
  |
2 | quoted_tuple: "(int, str)"  # snapshot: invalid-type-form
  |                ^^^^^^^^^^ Did you mean `tuple[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: Dict literals are not allowed in type expressions
 --> src/mdtest_snippet.py:3:15
  |
3 | quoted_dict: "{int: str}"  # snapshot: invalid-type-form
  |               ^^^^^^^^^^ Did you mean `dict[int, str]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions


error[invalid-type-form]: Set literals are not allowed in type expressions
 --> src/mdtest_snippet.py:4:14
  |
4 | quoted_set: "{int}"  # snapshot: invalid-type-form
  |              ^^^^^ Did you mean `set[int]`?
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

### Special-cased diagnostic for `callable` used in a type expression

<!-- snapshot-diagnostics -->

```py
# error: [invalid-type-form]
# error: [invalid-type-form]
def decorator(fn: callable) -> callable:
    return fn
```

### AST nodes that are only valid inside `Literal`

<!-- snapshot-diagnostics -->

```py
def bad(
    # error: [invalid-type-form]
    a: 42,
    # error: [invalid-type-form]
    b: b"42",
    # error: [invalid-type-form]
    c: True,
    # error: [invalid-syntax-in-forward-annotation]
    d: "invalid syntax",
): ...
```

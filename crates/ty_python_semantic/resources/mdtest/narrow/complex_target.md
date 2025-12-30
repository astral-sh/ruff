# Narrowing for complex targets (attribute expressions, subscripts)

We support type narrowing for attributes and subscripts.

## Attribute narrowing

### Basic

```py
from ty_extensions import Unknown

class C:
    x: int | None = None

c = C()

reveal_type(c.x)  # revealed: int | None

if c.x is not None:
    reveal_type(c.x)  # revealed: int
else:
    reveal_type(c.x)  # revealed: None

if c.x is not None:
    c.x = None

reveal_type(c.x)  # revealed: None

c = C()

if c.x is None:
    c.x = 1

reveal_type(c.x)  # revealed: int

class _:
    reveal_type(c.x)  # revealed: int

c = C()

class _:
    if c.x is None:
        c.x = 1
    reveal_type(c.x)  # revealed: int

# TODO: should be `int`
reveal_type(c.x)  # revealed: int | None

class D:
    x = None

def unknown() -> Unknown:
    return 1

d = D()
reveal_type(d.x)  # revealed: Unknown | None
d.x = 1
reveal_type(d.x)  # revealed: Literal[1]
d.x = unknown()
reveal_type(d.x)  # revealed: Unknown

class E:
    x: int | None = None

e = E()

if e.x is not None:
    class _:
        reveal_type(e.x)  # revealed: int
```

Narrowing can be "reset" by assigning to the attribute:

```py
c = C()

if c.x is None:
    reveal_type(c.x)  # revealed: None
    c.x = 1
    reveal_type(c.x)  # revealed: Literal[1]
    c.x = None
    reveal_type(c.x)  # revealed: None

reveal_type(c.x)  # revealed: int | None
```

Narrowing can also be "reset" by assigning to the object:

```py
c = C()

if c.x is None:
    reveal_type(c.x)  # revealed: None
    c = C()
    reveal_type(c.x)  # revealed: int | None

reveal_type(c.x)  # revealed: int | None
```

### Multiple predicates

```py
class C:
    value: str | None

def foo(c: C):
    # The truthiness check `c.value` narrows to `str & ~AlwaysFalsy`.
    # The subsequent `len(c.value)` doesn't narrow further since `str` is not narrowable by len().
    if c.value and len(c.value):
        reveal_type(c.value)  # revealed: str & ~AlwaysFalsy

    # error: [invalid-argument-type] "Argument to function `len` is incorrect: Expected `Sized`, found `str | None`"
    if len(c.value) and c.value:
        reveal_type(c.value)  # revealed: str & ~AlwaysFalsy

    if c.value is None or not len(c.value):
        reveal_type(c.value)  # revealed: str | None
    else:  # c.value is not None and len(c.value)
        # `c.value is not None` narrows to `str`, but `str` is not narrowable by len().
        reveal_type(c.value)  # revealed: str
```

### Generic class

```toml
[environment]
python-version = "3.12"
```

```py
class C[T]:
    x: T
    y: T

    def __init__(self, x: T):
        self.x = x
        self.y = x

def f(a: int | None):
    c = C(a)
    reveal_type(c.x)  # revealed: int | None
    reveal_type(c.y)  # revealed: int | None
    if c.x is not None:
        reveal_type(c.x)  # revealed: int
        # In this case, it may seem like we can narrow it down to `int`,
        # but different values ​​may be reassigned to `x` and `y` in another place.
        reveal_type(c.y)  # revealed: int | None

def g[T](c: C[T]):
    reveal_type(c.x)  # revealed: T@g
    reveal_type(c.y)  # revealed: T@g
    reveal_type(c)  # revealed: C[T@g]

    if isinstance(c.x, int):
        reveal_type(c.x)  # revealed: T@g & int
        reveal_type(c.y)  # revealed: T@g
        reveal_type(c)  # revealed: C[T@g]
    if isinstance(c.x, int) and isinstance(c.y, int):
        reveal_type(c.x)  # revealed: T@g & int
        reveal_type(c.y)  # revealed: T@g & int
        # TODO: Probably better if inferred as `C[T & int]` (mypy and pyright don't support this)
        reveal_type(c)  # revealed: C[T@g]
```

### With intermediate scopes

```py
class C:
    def __init__(self):
        self.x: int | None = None
        self.y: int | None = None

c = C()
reveal_type(c.x)  # revealed: int | None
if c.x is not None:
    reveal_type(c.x)  # revealed: int
    reveal_type(c.y)  # revealed: int | None

if c.x is not None:
    def _():
        reveal_type(c.x)  # revealed: int | None

def _():
    if c.x is not None:
        reveal_type(c.x)  # revealed: int
```

## Subscript narrowing

### Number subscript

```py
def _(t1: tuple[int | None, int | None], t2: tuple[int, int] | tuple[None, None]):
    if t1[0] is not None:
        reveal_type(t1[0])  # revealed: int
        reveal_type(t1[1])  # revealed: int | None

    n = 0
    if t1[n] is not None:
        # Narrowing the individual element type with a non-literal subscript is not supported
        reveal_type(t1[0])  # revealed: int | None
        reveal_type(t1[n])  # revealed: int | None
        reveal_type(t1[1])  # revealed: int | None

    # However, we can still discriminate between tuples in a union using a variable index:
    if t2[n] is not None:
        reveal_type(t2)  # revealed: tuple[int, int]

    if t2[0] is not None:
        reveal_type(t2)  # revealed: tuple[int, int]
        reveal_type(t2[0])  # revealed: int
        reveal_type(t2[1])  # revealed: int
    else:
        reveal_type(t2)  # revealed: tuple[None, None]
        reveal_type(t2[0])  # revealed: None
        reveal_type(t2[1])  # revealed: None

    if t2[0] is None:
        reveal_type(t2)  # revealed: tuple[None, None]
    else:
        reveal_type(t2)  # revealed: tuple[int, int]

def _(t3: tuple[int, str] | tuple[None, None] | tuple[bool, bytes]):
    # Narrow to tuples where first element is not None
    if t3[0] is not None:
        reveal_type(t3)  # revealed: tuple[int, str] | tuple[bool, bytes]

    # Narrow to tuples where first element is None
    if t3[0] is None:
        reveal_type(t3)  # revealed: tuple[None, None]

def _(t4: tuple[bool, int] | tuple[bool, str]):
    # Both tuples have bool at index 0, which is not disjoint from True,
    # so neither gets filtered out when checking `is True`
    if t4[0] is True:
        reveal_type(t4)  # revealed: tuple[bool, int] | tuple[bool, str]

def _(t5: tuple[int, None] | tuple[None, int]):
    # Narrow on second element (index 1)
    if t5[1] is not None:
        reveal_type(t5)  # revealed: tuple[None, int]
    else:
        reveal_type(t5)  # revealed: tuple[int, None]

    # Negative index
    if t5[-1] is None:
        reveal_type(t5)  # revealed: tuple[int, None]

def _(t6: tuple[int, ...] | tuple[None, None]):
    # Variadic tuple at index 0 has element type `int` (not a union),
    # so `tuple[None, None]` gets filtered out
    if t6[0] is not None:
        reveal_type(t6)  # revealed: tuple[int, ...]

def _(t6b: tuple[int, ...] | tuple[None, ...]):
    # Both variadic: `int` is disjoint from None, `None` is not disjoint from None
    if t6b[0] is not None:
        reveal_type(t6b)  # revealed: tuple[int, ...]
    else:
        reveal_type(t6b)  # revealed: tuple[None, ...]

def _(t7: tuple[int, int] | tuple[None, None]):
    # Index out of range for both tuples - no narrowing, but errors are emitted
    # error: [index-out-of-bounds] "Index 5 is out of bounds for tuple `tuple[int, int]` with length 2"
    # error: [index-out-of-bounds] "Index 5 is out of bounds for tuple `tuple[None, None]` with length 2"
    if t7[5] is not None:
        reveal_type(t7)  # revealed: tuple[int, int] | tuple[None, None]

def _(t8: tuple[int, int, int] | tuple[None, None]):
    # Index in range for first tuple but out of range for second
    # error: [index-out-of-bounds] "Index 2 is out of bounds for tuple `tuple[None, None]` with length 2"
    if t8[2] is not None:
        reveal_type(t8)  # revealed: tuple[int, int, int] | tuple[None, None]

def _(t9: tuple[int | None, str] | tuple[str, int]):
    # When the element type is a union (like `int | None`), we can't filter
    # out the tuple.
    if t9[0] is not None:
        reveal_type(t9)  # revealed: tuple[int | None, str] | tuple[str, int]
```

### String subscript

```py
def _(d: dict[str, str | None]):
    if d["a"] is not None:
        reveal_type(d["a"])  # revealed: str
        reveal_type(d["b"])  # revealed: str | None
```

## Combined attribute and subscript narrowing

```py
class C:
    def __init__(self):
        self.x: tuple[int | None, int | None] = (None, None)

class D:
    def __init__(self):
        self.c: tuple[C] | None = None

d = D()
if d.c is not None and d.c[0].x[0] is not None:
    reveal_type(d.c[0].x[0])  # revealed: int
```

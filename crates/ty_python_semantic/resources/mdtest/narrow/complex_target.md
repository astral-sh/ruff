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
    if c.value and len(c.value):
        reveal_type(c.value)  # revealed: str & ~AlwaysFalsy

    # error: [invalid-argument-type] "Argument to function `len` is incorrect: Expected `Sized`, found `str | None`"
    if len(c.value) and c.value:
        reveal_type(c.value)  # revealed: str & ~AlwaysFalsy

    if c.value is None or not len(c.value):
        reveal_type(c.value)  # revealed: str | None
    else:  # c.value is not None and len(c.value)
        # TODO: should be # `str & ~AlwaysFalsy`
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
    reveal_type(c.x)  # revealed: T
    reveal_type(c.y)  # revealed: T
    reveal_type(c)  # revealed: C[T]

    if isinstance(c.x, int):
        reveal_type(c.x)  # revealed: T & int
        reveal_type(c.y)  # revealed: T
        reveal_type(c)  # revealed: C[T]
    if isinstance(c.x, int) and isinstance(c.y, int):
        reveal_type(c.x)  # revealed: T & int
        reveal_type(c.y)  # revealed: T & int
        # TODO: Probably better if inferred as `C[T & int]` (mypy and pyright don't support this)
        reveal_type(c)  # revealed: C[T]
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
        reveal_type(c.x)  # revealed: Unknown | int | None

def _():
    if c.x is not None:
        reveal_type(c.x)  # revealed: (Unknown & ~None) | int
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
        # Non-literal subscript narrowing are currently not supported, as well as mypy, pyright
        reveal_type(t1[0])  # revealed: int | None
        reveal_type(t1[n])  # revealed: int | None
        reveal_type(t1[1])  # revealed: int | None

    if t2[0] is not None:
        reveal_type(t2[0])  # revealed: int
        # TODO: should be int
        reveal_type(t2[1])  # revealed: int | None
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

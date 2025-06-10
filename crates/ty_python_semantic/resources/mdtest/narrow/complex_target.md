# Narrowing for complex targets (attribute expressions, subscripts)

We support type narrowing for attributes and subscripts.

## Basic attribute narrowing

```py
class C:
    x: int | None = None

c = C()

reveal_type(c.x)  # revealed: int | None

if c.x is not None:
    reveal_type(c.x)  # revealed: int
else:
    reveal_type(c.x)  # revealed: None
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

# TODO: this should be int | None
reveal_type(c.x)  # revealed: int
```

Narrowing can also be "reset" by assigning to the object:

```py
c = C()

if c.x is None:
    reveal_type(c.x)  # revealed: None
    c = C()
    reveal_type(c.x)  # revealed: int | None

# TODO: this should be int | None
reveal_type(c.x)  # revealed: int
```

## Attribute narrowing with intermediate scopes

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
        # TODO: should be int
        reveal_type(t2[0])  # revealed: Unknown & ~None
        # TODO: should be int
        reveal_type(t2[1])  # revealed: Unknown
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

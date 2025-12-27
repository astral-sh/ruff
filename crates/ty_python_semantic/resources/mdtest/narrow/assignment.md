# Narrowing by assignment

## Attribute

### Basic

```py
class A:
    x: int | None = None
    y = None

    def __init__(self):
        self.z = None

a = A()
a.x = 0
a.y = 0
a.z = 0

reveal_type(a.x)  # revealed: Literal[0]
reveal_type(a.y)  # revealed: Literal[0]
reveal_type(a.z)  # revealed: Literal[0]

# Make sure that we infer the narrowed type for eager
# scopes (class, comprehension) and the non-narrowed
# public type for lazy scopes (function)
class _:
    reveal_type(a.x)  # revealed: Literal[0]
    reveal_type(a.y)  # revealed: Literal[0]
    reveal_type(a.z)  # revealed: Literal[0]

[reveal_type(a.x) for _ in range(1)]  # revealed: Literal[0]
[reveal_type(a.y) for _ in range(1)]  # revealed: Literal[0]
[reveal_type(a.z) for _ in range(1)]  # revealed: Literal[0]

def _():
    reveal_type(a.x)  # revealed: int | None
    reveal_type(a.y)  # revealed: Unknown | None
    reveal_type(a.z)  # revealed: Unknown | None

if False:
    a = A()
reveal_type(a.x)  # revealed: Literal[0]
reveal_type(a.y)  # revealed: Literal[0]
reveal_type(a.z)  # revealed: Literal[0]

if True:
    a = A()
reveal_type(a.x)  # revealed: int | None
reveal_type(a.y)  # revealed: Unknown | None
reveal_type(a.z)  # revealed: Unknown | None

a.x = 0
a.y = 0
a.z = 0
reveal_type(a.x)  # revealed: Literal[0]
reveal_type(a.y)  # revealed: Literal[0]
reveal_type(a.z)  # revealed: Literal[0]

class _:
    a = A()
    reveal_type(a.x)  # revealed: int | None
    reveal_type(a.y)  # revealed: Unknown | None
    reveal_type(a.z)  # revealed: Unknown | None

def cond() -> bool:
    return True

class _:
    if False:
        a = A()
    reveal_type(a.x)  # revealed: Literal[0]
    reveal_type(a.y)  # revealed: Literal[0]
    reveal_type(a.z)  # revealed: Literal[0]

    if cond():
        a = A()
    reveal_type(a.x)  # revealed: int | None
    reveal_type(a.y)  # revealed: Unknown | None
    reveal_type(a.z)  # revealed: Unknown | None

class _:
    a = A()

    class Inner:
        reveal_type(a.x)  # revealed: int | None
        reveal_type(a.y)  # revealed: Unknown | None
        reveal_type(a.z)  # revealed: Unknown | None

a = A()
# error: [unresolved-attribute]
a.dynamically_added = 0
# error: [unresolved-attribute]
reveal_type(a.dynamically_added)  # revealed: Literal[0]

# error: [unresolved-reference]
does.nt.exist = 0
# error: [unresolved-reference]
reveal_type(does.nt.exist)  # revealed: Unknown
```

### Narrowing chain

```py
class D: ...

class C:
    d: D | None = None

class B:
    c1: C | None = None
    c2: C | None = None

class A:
    b: B | None = None

a = A()
a.b = B()
a.b.c1 = C()
a.b.c2 = C()
a.b.c1.d = D()
a.b.c2.d = D()
reveal_type(a.b)  # revealed: B
reveal_type(a.b.c1)  # revealed: C
reveal_type(a.b.c1.d)  # revealed: D

a.b.c1 = C()
reveal_type(a.b)  # revealed: B
reveal_type(a.b.c1)  # revealed: C
reveal_type(a.b.c1.d)  # revealed: D | None
reveal_type(a.b.c2.d)  # revealed: D

a.b.c1.d = D()
a.b = B()
reveal_type(a.b)  # revealed: B
reveal_type(a.b.c1)  # revealed: C | None
reveal_type(a.b.c2)  # revealed: C | None
# error: [possibly-missing-attribute]
reveal_type(a.b.c1.d)  # revealed: D | None
# error: [possibly-missing-attribute]
reveal_type(a.b.c2.d)  # revealed: D | None
```

### Do not narrow the type of a `property` by assignment

```py
class C:
    def __init__(self):
        self._x: int = 0

    @property
    def x(self) -> int:
        return self._x

    @x.setter
    def x(self, value: int) -> None:
        self._x = abs(value)

c = C()
c.x = -1
# Don't infer `c.x` to be `Literal[-1]`
reveal_type(c.x)  # revealed: int
```

### Do not narrow the type of a descriptor by assignment

```py
class Descriptor:
    def __get__(self, instance: object, owner: type) -> int:
        return 1

    def __set__(self, instance: object, value: int) -> None:
        pass

class C:
    desc: Descriptor = Descriptor()

c = C()
c.desc = -1
# Don't infer `c.desc` to be `Literal[-1]`
reveal_type(c.desc)  # revealed: int
```

## Subscript

### Specialization for builtin types

Type narrowing based on assignment to a subscript expression is generally unsound, because arbitrary
`__getitem__`/`__setitem__` methods on a class do not necessarily guarantee that the passed-in value
for `__setitem__` is stored and can be retrieved unmodified via `__getitem__`. Therefore, we
currently only perform assignment-based narrowing on a few built-in classes (`list`, `dict`,
`bytesarray`, `TypedDict` and `collections` types) where we are confident that this kind of
narrowing can be performed soundly. This is the same approach as pyright.

```py
from typing import TypedDict
from collections import ChainMap, defaultdict

l: list[int | None] = [None]
l[0] = 0
d: dict[int, int] = {1: 1}
d[0] = 0
b: bytearray = bytearray(b"abc")
b[0] = 0
dd: defaultdict[int, int] = defaultdict(int)
dd[0] = 0
cm: ChainMap[int, int] = ChainMap({1: 1}, {0: 0})
cm[0] = 0
reveal_type(cm)  # revealed: ChainMap[int, int]

reveal_type(l[0])  # revealed: Literal[0]
reveal_type(d[0])  # revealed: Literal[0]
reveal_type(b[0])  # revealed: Literal[0]
reveal_type(dd[0])  # revealed: Literal[0]
reveal_type(cm[0])  # revealed: Literal[0]

class C:
    reveal_type(l[0])  # revealed: Literal[0]
    reveal_type(d[0])  # revealed: Literal[0]
    reveal_type(b[0])  # revealed: Literal[0]
    reveal_type(dd[0])  # revealed: Literal[0]
    reveal_type(cm[0])  # revealed: Literal[0]

[reveal_type(l[0]) for _ in range(1)]  # revealed: Literal[0]
[reveal_type(d[0]) for _ in range(1)]  # revealed: Literal[0]
[reveal_type(b[0]) for _ in range(1)]  # revealed: Literal[0]
[reveal_type(dd[0]) for _ in range(1)]  # revealed: Literal[0]
[reveal_type(cm[0]) for _ in range(1)]  # revealed: Literal[0]

def _():
    reveal_type(l[0])  # revealed: int | None
    reveal_type(d[0])  # revealed: int
    reveal_type(b[0])  # revealed: int
    reveal_type(dd[0])  # revealed: int
    reveal_type(cm[0])  # revealed: int

class D(TypedDict):
    x: int
    label: str

td = D(x=1, label="a")
td["x"] = 0
reveal_type(td["x"])  # revealed: Literal[0]

# error: [unresolved-reference]
does["not"]["exist"] = 0
# error: [unresolved-reference]
reveal_type(does["not"]["exist"])  # revealed: Unknown

not_subscriptable = 1
# error: [invalid-assignment]
not_subscriptable[0] = 0
# error: [not-subscriptable]
reveal_type(not_subscriptable[0])  # revealed: Unknown
```

### No narrowing for custom classes with arbitrary `__getitem__` / `__setitem__`

```py
class C:
    def __init__(self):
        self.l: list[str] = []

    def __getitem__(self, index: int) -> str:
        return self.l[index]

    def __setitem__(self, index: int, value: str | int) -> None:
        if len(self.l) == index:
            self.l.append(str(value))
        else:
            self.l[index] = str(value)

c = C()
c[0] = 0
reveal_type(c[0])  # revealed: str
```

## Complex target

```py
class A:
    x: list[int | None] = []

class B:
    a: A | None = None

b = B()
b.a = A()
b.a.x[0] = 0

reveal_type(b.a.x[0])  # revealed: Literal[0]

class C:
    reveal_type(b.a.x[0])  # revealed: Literal[0]

def _():
    # error: [possibly-missing-attribute]
    reveal_type(b.a.x[0])  # revealed: int | None
    # error: [possibly-missing-attribute]
    reveal_type(b.a.x)  # revealed: list[int | None]
    reveal_type(b.a)  # revealed: A | None
```

## Invalid assignments are not used for narrowing

```py
class C:
    x: int | None
    l: list[int]

def f(c: C, s: str):
    c.x = s  # error: [invalid-assignment]
    reveal_type(c.x)  # revealed: int | None
    s = c.x  # error: [invalid-assignment]

    # error: [invalid-assignment] "Invalid subscript assignment with key of type `Literal[0]` and value of type `str` on object of type `list[int]`"
    c.l[0] = s
    reveal_type(c.l[0])  # revealed: int
```

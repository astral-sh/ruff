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
    reveal_type(a.y)  # revealed: None | Unknown
    reveal_type(a.z)  # revealed: None | Unknown

if False:
    a = A()
reveal_type(a.x)  # revealed: Literal[0]
reveal_type(a.y)  # revealed: Literal[0]
reveal_type(a.z)  # revealed: Literal[0]

if True:
    a = A()
reveal_type(a.x)  # revealed: int | None
reveal_type(a.y)  # revealed: None | Unknown
reveal_type(a.z)  # revealed: None | Unknown

a.x = 0
a.y = 0
a.z = 0
reveal_type(a.x)  # revealed: Literal[0]
reveal_type(a.y)  # revealed: Literal[0]
reveal_type(a.z)  # revealed: Literal[0]

class _:
    a = A()
    reveal_type(a.x)  # revealed: int | None
    reveal_type(a.y)  # revealed: None | Unknown
    reveal_type(a.z)  # revealed: None | Unknown

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
    reveal_type(a.y)  # revealed: None | Unknown
    reveal_type(a.z)  # revealed: None | Unknown

class _:
    a = A()

    class Inner:
        reveal_type(a.x)  # revealed: int | None
        reveal_type(a.y)  # revealed: None | Unknown
        reveal_type(a.z)  # revealed: None | Unknown

a = A()
# error: [unresolved-attribute]
a.dynamically_added = 0
# error: [unresolved-attribute]
reveal_type(a.dynamically_added)  # revealed: Literal[0]

# error: [unresolved-reference]
does.nt.exist = 0
# error: [unresolved-reference]
reveal_type(does.nt.exist)  # revealed: Literal[0]
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
# error: [unresolved-attribute]
reveal_type(a.b.c1.d)  # revealed: D | None
# error: [unresolved-attribute]
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

### Dynamic attributes

An attribute provided by `__getattr__` retains its read type after assignment. A custom
`__setattr__` can transform or discard the assigned value, so the assignment does not determine what
a subsequent read returns.

```py
from typing import Any

class Props:
    def __getattr__(self, name: str) -> Any: ...
    def __setattr__(self, name: str, value: object) -> None: ...

def update(props: Props) -> None:
    props.status = "Closed"
    reveal_type(props.status)  # revealed: Any
    props.status.name  # no diagnostic

    if hasattr(props, "other"):
        props.status = "Open"
        reveal_type(props.status)  # revealed: Any

    if hasattr(props, "status"):
        props.status = "Open"
        reveal_type(props.status)  # revealed: Any
```

The same rule applies when the getter returns a static type.

```py
class Values:
    def __getattr__(self, name: str) -> int | str:
        return 0

    def __setattr__(self, name: str, value: object) -> None: ...

def update(values: Values) -> None:
    values.value = 1
    reveal_type(values.value)  # revealed: int | str

    if isinstance(values.value, int):
        reveal_type(values.value)  # revealed: int
        values.value = 2
        reveal_type(values.value)  # revealed: int | str
```

The getter also determines the read type when there is no custom setter. The assignment is still
invalid because the class does not provide the attribute or a setter that accepts it.

```py
class ReadOnlyProps:
    def __getattr__(self, name: str) -> Any: ...

def update(props: ReadOnlyProps) -> None:
    props.status = "Closed"  # error: [unresolved-attribute]
    reveal_type(props.status)  # revealed: Any
    props.status.name  # no diagnostic
```

### Known attributes with dynamic access

Declared and inferred attributes still narrow after assignment, even when the class has custom
`__getattr__` and `__setattr__` methods. This preserves narrowing for ordinary fields on classes
that also provide dynamic attributes.

```py
from typing import Any

class Base:
    cache: list[int] | None = None
    inferred = None

    def __init__(self) -> None:
        self.instance = None

    def __getattr__(self, name: str) -> Any: ...
    def __setattr__(self, name: str, value: object) -> None: ...

class Child(Base): ...

def update(obj: Child) -> None:
    if obj.cache is None:
        obj.cache = []
    reveal_type(obj.cache)  # revealed: list[int]
    obj.cache.append(1)  # no diagnostic

    obj.inferred = 1
    reveal_type(obj.inferred)  # revealed: Literal[1]
    obj.instance = 1
    reveal_type(obj.instance)  # revealed: Literal[1]

    if hasattr(obj, "other"):
        obj.cache = []
        reveal_type(obj.cache)  # revealed: list[int]
```

### Dynamic attributes on union receivers

An assignment cannot narrow an attribute when one of the possible receivers supplies it through
`__getattr__`.

```py
class Dynamic:
    def __getattr__(self, name: str) -> int | str:
        return 0

    def __setattr__(self, name: str, value: object) -> None: ...

class Declared:
    value: int | str

def update(obj: Dynamic | Declared) -> None:
    obj.value = 1
    reveal_type(obj.value)  # revealed: int | str
```

### Module attributes

We retain the assigned type after an invalid write to a missing module attribute. The synthetic
`ModuleType.__getattr__` in typeshed does not provide attributes on a statically known module.

`module.py`:

```py
```

`main.py`:

```py
import module

module.value = 1  # error: [unresolved-attribute]
# error: [unresolved-attribute]
reveal_type(module.value)  # revealed: Literal[1]

# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to `str`"
# error: [unresolved-attribute]
value: str = module.value
```

### Module attributes with a dynamic getter

Assigning a module attribute creates a value that shadows its module-level `__getattr__`. Subsequent
reads retain the assigned type.

`module.py`:

```py
def __getattr__(name: str) -> int | str:
    return 0
```

`main.py`:

```py
import module

reveal_type(module.value)  # revealed: int | str
module.value = 1
reveal_type(module.value)  # revealed: Literal[1]
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
from typing import Any

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
    # error: [unresolved-attribute]
    reveal_type(b.a.x[0])  # revealed: int | None
    # error: [unresolved-attribute]
    reveal_type(b.a.x)  # revealed: list[int | None]
    reveal_type(b.a)  # revealed: A | None

class D: ...

class E:
    def __init__(self):
        self.d = D()

class F:
    def __init__(self):
        self.e = E()

class Mock(Any): ...

f = F()
reveal_type(f.e)  # revealed: E
f.e = Mock()
reveal_type(f.e)  # revealed: Mock

f2 = F()
reveal_type(f2.e.d)  # revealed: D
f2.e.d = Mock()
reveal_type(f2.e.d)  # revealed: Mock
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

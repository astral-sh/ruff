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
# The assignment is invalid, but establishes the attribute's presence for subsequent reads.
reveal_type(a.dynamically_added)  # revealed: Literal[0]

# error: [unresolved-reference]
does.nt.exist = 0
# error: [unresolved-reference]
reveal_type(does.nt.exist)  # revealed: Literal[0]
```

### Presence after conditional assignments

Assigning an undeclared attribute is still an error, but a subsequent read does not repeat the error
when every branch assigns the attribute.

```py
class Item: ...

def assigned_on_both_branches(item: Item, condition: bool):
    if condition:
        item.value = 1  # error: [unresolved-attribute]
    else:
        item.value = 2  # error: [unresolved-attribute]
    reveal_type(item.value)  # revealed: Literal[1, 2]
```

An assignment on only one branch does not establish presence after the conditional.

```py
def assigned_on_one_branch(item: Item, condition: bool):
    if condition:
        item.value = 1  # error: [unresolved-attribute]
    item.value  # error: [unresolved-attribute]
```

A branch that exits without reaching the read does not affect whether the attribute is present.

```py
def assigned_or_raised(item: Item, condition: bool):
    if condition:
        item.value = 1  # error: [unresolved-attribute]
    else:
        raise ValueError
    reveal_type(item.value)  # revealed: Literal[1]
```

### Presence across calls and branch joins

A call can mutate an object. Its arguments observe the preceding assignment, but later reads need
new presence evidence. Forgetting presence preserves the inferred value type.

```py
class Item: ...

def mutate(item: object) -> None: ...
def after_call(item: Item):
    item.value = 1  # error: [unresolved-attribute]
    mutate(item.value)
    # error: [unresolved-attribute]
    reveal_type(item.value)  # revealed: Literal[1]
    assert hasattr(item, "value")
    item.value
```

Each branch can establish presence independently, through either an assignment or a guard.

```py
def separate_proofs(item: Item, condition: bool):
    if condition:
        item.value = 1  # error: [unresolved-attribute]
    else:
        assert hasattr(item, "value")
    item.value

def renewed_proof(item: Item, condition: bool):
    item.value = 1  # error: [unresolved-attribute]
    if condition:
        mutate(item)
        assert hasattr(item, "value")
    item.value

def missing_proof(item: Item, condition: bool):
    item.value = 1  # error: [unresolved-attribute]
    if condition:
        mutate(item)
    item.value  # error: [unresolved-attribute]
```

A call can mutate an object before raising, so its exception handler also needs fresh evidence. An
assignment in the handler can establish presence for the join with the successful path.

```py
def caught_mutation(item: Item):
    item.value = 1  # error: [unresolved-attribute]
    try:
        mutate(item)
    except Exception:
        item.value  # error: [unresolved-attribute]

def renewed_in_handler(item: Item):
    try:
        mutate(item)
        item.value = 1  # error: [unresolved-attribute]
    except Exception:
        item.value = 2  # error: [unresolved-attribute]
    item.value
```

### Presence when an eager scope raises

An eager class body can delete an attribute and then raise before the enclosing scope resumes. The
handler cannot rely on the assignment from before the class body.

```py
class Item: ...

def eager_exception(item: Item):
    item.value = 1  # error: [unresolved-attribute]
    try:
        class Inner:
            del item.value
            raise ValueError

    except Exception:
        item.value  # error: [unresolved-attribute]
```

An attribute first read after a call in a class body cannot inherit stale presence. A new local
assignment establishes presence even if an enclosing scope previously discarded its proof.

```py
def mutate(item: Item) -> None: ...
def first_read_after_call(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        mutate(item)
        item.value  # error: [unresolved-attribute]

def assigned_in_class(item: Item):
    item.value = 1  # error: [unresolved-attribute]
    mutate(item)

    class Inner:
        item.value = 2  # error: [unresolved-attribute]
        item.value
```

### Presence after deletion in an eager scope

An assignment in an enclosing scope establishes presence in an eager class body. Deleting the
attribute in that class body invalidates this evidence, so a later read reports the missing
attribute.

```py
class Item: ...

item = Item()
item.value = 1  # error: [unresolved-attribute]

class Inner:
    item.value
    del item.value
    item.value  # error: [unresolved-attribute]
```

A conditional deletion also invalidates presence after the branches join. Assigning the attribute
again establishes presence for subsequent reads.

```py
def conditional_deletion(item: Item, condition: bool):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        if condition:
            del item.value
        item.value  # error: [unresolved-attribute]
        item.value = 2  # error: [unresolved-attribute]
        reveal_type(item.value)  # revealed: Literal[2]
```

An unreachable deletion does not invalidate presence from the enclosing scope.

```py
def unreachable_deletion(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        if False:
            del item.value
        reveal_type(item.value)  # revealed: Literal[1]
```

### Presence after deletion across eager scopes

A comprehension inside a class body observes deletions made before it runs. An earlier assignment in
the enclosing function does not establish presence after that deletion.

```py
class Item: ...

def deleted_before_comprehension(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        del item.value
        [item.value for _ in range(1)]  # error: [unresolved-attribute]
```

An unreachable deletion preserves the assigned type. The call that constructs the iterable clears
presence evidence before the comprehension runs.

```py
def unreachable_deletion_before_comprehension(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        if False:
            del item.value
        # error: [unresolved-attribute]
        [reveal_type(item.value) for _ in range(1)]  # revealed: Literal[1]
```

### Presence after deletion across loop iterations

A read before a deletion can still observe that deletion on a later iteration. An assignment before
the class body does not establish that the attribute is present on every iteration.

```py
class Item: ...

def deleted_on_previous_iteration(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        for _ in range(2):
            item.value  # error: [unresolved-attribute]
            del item.value  # error: [unresolved-attribute]
```

The deletion also affects a read in a comprehension nested inside the loop.

```py
def deleted_on_previous_iteration_in_comprehension(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        for _ in range(2):
            [item.value for _ in range(1)]  # error: [unresolved-attribute]
            del item.value  # error: [unresolved-attribute]
```

Assigning the attribute before each read establishes presence again, even when the previous
iteration deleted it.

```py
def assigned_on_each_iteration(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        for _ in range(2):
            item.value = 2  # error: [unresolved-attribute]
            item.value
            del item.value
```

### Presence after receiver reassignment

Reassigning the receiver discards evidence that an attribute was assigned on the previous object.

```py
class Item: ...

def f(item: Item, other: Item):
    item.value = 1  # error: [unresolved-attribute]
    reveal_type(item.value)  # revealed: Literal[1]
    item = other
    item.value  # error: [unresolved-attribute]
```

### Presence after an eager scope exits

A class body executes before the enclosing scope continues. Deleting an attribute in the class body
invalidates an earlier assignment in the enclosing scope. Assigning it again restores presence.

```py
class Item: ...

def deleted_in_class(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        item.value
        del item.value

    item.value  # error: [unresolved-attribute]
    [item.value for _ in range(1)]  # error: [unresolved-attribute]
    item.value = 2  # error: [unresolved-attribute]
    reveal_type(item.value)  # revealed: Literal[2]
```

The invalidation also reaches enclosing scopes through nested class bodies.

```py
def deleted_in_nested_class(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Outer:
        class Inner:
            del item.value

        item.value  # error: [unresolved-attribute]

    item.value  # error: [unresolved-attribute]
```

A conditional deletion can leave the attribute missing. Eager scope boundaries conservatively clear
presence even when the mutation is unreachable; the assigned type is preserved.

```py
def conditionally_deleted_in_class(item: Item, condition: bool):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        if condition:
            del item.value

    item.value  # error: [unresolved-attribute]

def unreachable_deletion_in_class(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    class Inner:
        if False:
            del item.value

    # error: [unresolved-attribute]
    reveal_type(item.value)  # revealed: Literal[1]
```

### Presence after receiver reassignment in an eager scope

Replacing a member invalidates assignments to attributes of the previous object.

```py
class Item: ...

class Box:
    item: Item

def replaced_in_class(box: Box):
    box.item.value = 1  # error: [unresolved-attribute]

    class Inner:
        box.item = Item()

    box.item.value  # error: [unresolved-attribute]
```

### Eager mutations across enclosing loop iterations

A class body can delete an attribute before the next iteration reads it.

```py
class Item: ...

def deleted_in_loop(item: Item):
    item.value = 1  # error: [unresolved-attribute]
    for _ in range(2):
        item.value  # error: [unresolved-attribute]

        class Inner:
            del item.value  # error: [unresolved-attribute]
```

### Conservative eager scope boundaries

A class-local receiver refers to a different object. We preserve the enclosing assignment's type,
but conservatively discard presence when the class body finishes.

```py
class Item: ...

item = Item()
item.value = 1  # error: [unresolved-attribute]

class Inner:
    item = Item()
    item.value = 2  # error: [unresolved-attribute]
    del item.value

# error: [unresolved-attribute]
reveal_type(item.value)  # revealed: Literal[1]
```

A loop around the class body also clears presence.

```py
for _ in range(2):
    class InLoop:
        item = Item()
        item.value = 2  # error: [unresolved-attribute]
        del item.value

    # error: [unresolved-attribute]
    reveal_type(item.value)  # revealed: Literal[1]
```

A function body does not execute when the function is defined, including class bodies nested inside
it.

```py
def outer(item: Item):
    item.value = 1  # error: [unresolved-attribute]

    def deferred():
        class Inner:
            del item.value  # error: [unresolved-attribute]

    reveal_type(item.value)  # revealed: Literal[1]
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

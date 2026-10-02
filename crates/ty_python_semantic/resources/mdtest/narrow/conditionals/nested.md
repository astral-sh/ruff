# Narrowing for nested conditionals

## Multiple negative contributions

```py
def _(x: int):
    if x != 1:
        if x != 2:
            if x != 3:
                reveal_type(x)  # revealed: int & ~Literal[1] & ~Literal[True] & ~Literal[2] & ~Literal[3]
```

## Multiple negative contributions with simplification

```py
from typing import Literal

def _(x: Literal[1, 2, 3]):
    if x != 1:
        reveal_type(x)  # revealed: Literal[2, 3]
        if x != 2:
            reveal_type(x)  # revealed: Literal[3]
```

## elif-else blocks

```py
from typing import Literal

def _(x: Literal[1, 2, 3]):
    if x != 1:
        reveal_type(x)  # revealed: Literal[2, 3]
        if x == 2:
            reveal_type(x)  # revealed: Literal[2]
        elif x == 3:
            reveal_type(x)  # revealed: Literal[3]
        else:
            reveal_type(x)  # revealed: Never

    elif x != 2:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: Never
```

## Comprehensions

```py
def _(xs: list[int | None], ys: list[str | bytes], list_of_optional_lists: list[list[int | None] | None]):
    [reveal_type(x) for x in xs if x is not None]  # revealed: int
    [reveal_type(y) for y in ys if isinstance(y, str)]  # revealed: str

    [_ for x in xs if x is not None if reveal_type(x) // 3 != 0]  # revealed: int

    # revealed: int & ~Literal[0] & ~Literal[False] & ~Literal[1] & ~Literal[True]
    [reveal_type(x) for x in xs if x is not None if x != 0 if x != 1]

    [reveal_type((x, y)) for x in xs if x is not None for y in ys if isinstance(y, str)]  # revealed: tuple[int, str]
    [reveal_type((x, y)) for y in ys if isinstance(y, str) for x in xs if x is not None]  # revealed: tuple[int, str]

    [reveal_type(i) for inner in list_of_optional_lists if inner is not None for i in inner if i is not None]  # revealed: int
```

## Cross-scope narrowing

Narrowing constraints are also valid in eager nested scopes (however, because class variables are
not visible from nested scopes, constraints on those variables are invalid).

Currently they are assumed to be invalid in lazy nested scopes since there is a possibility that the
constraints may no longer be valid due to a "time lag". However, it may be possible to determine
that some of them are valid by performing a more detailed analysis (e.g. checking that the narrowing
target has not changed in all places where the function is called).

### Narrowing by attribute/subscript assignments

```py
class A:
    x: str | None = None

    def update_x(self, value: str | None):
        self.x = value

a = A()
a.x = "a"

class B:
    reveal_type(a.x)  # revealed: Literal["a"]

def f():
    reveal_type(a.x)  # revealed: str | None

[reveal_type(a.x) for _ in range(1)]  # revealed: Literal["a"]

a = A()

class C:
    reveal_type(a.x)  # revealed: str | None

def g():
    reveal_type(a.x)  # revealed: str | None

[reveal_type(a.x) for _ in range(1)]  # revealed: str | None

a = A()
a.x = "a"
a.update_x("b")

class D:
    # TODO: should be `str | None`
    reveal_type(a.x)  # revealed: Literal["a"]

def h():
    reveal_type(a.x)  # revealed: str | None

# TODO: should be `str | None`
[reveal_type(a.x) for _ in range(1)]  # revealed: Literal["a"]
```

### Narrowing by attribute/subscript assignments in nested scopes

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

class _:
    a.b.c1 = C()

    class _:
        a.b.c1.d = D()
        a = 1

        class _3:
            reveal_type(a)  # revealed: A
            reveal_type(a.b.c1.d)  # revealed: D

    class _:
        a = 1
        # error: [unresolved-attribute]
        a.b.c1.d = D()

        class _3:
            # The assignment above uses the class-local `a`; this scope sees the outer `a`.
            reveal_type(a)  # revealed: A
            reveal_type(a.b.c1.d)  # revealed: D | None

a.b.c1 = C()
a.b.c1.d = D()

class _:
    a.b = B()

    class _:
        # error: [unresolved-attribute]
        reveal_type(a.b.c1.d)  # revealed: D | None
        reveal_type(a.b.c1)  # revealed: C | None
```

### Narrowing constraints introduced in eager nested scopes

```py
g: str | None = "a"

class A:
    x: str | None = None

a = A()

l: list[str | None] = [None]

def f(x: str | None):
    def _():
        if x is not None:
            reveal_type(x)  # revealed: str

        if not isinstance(x, str):
            reveal_type(x)  # revealed: None

        if g is not None:
            reveal_type(g)  # revealed: str

        if a.x is not None:
            reveal_type(a.x)  # revealed: str

        if l[0] is not None:
            reveal_type(l[0])  # revealed: str

    class C:
        if x is not None:
            reveal_type(x)  # revealed: str

        if not isinstance(x, str):
            reveal_type(x)  # revealed: None

        if g is not None:
            reveal_type(g)  # revealed: str

        if a.x is not None:
            reveal_type(a.x)  # revealed: str

        if l[0] is not None:
            reveal_type(l[0])  # revealed: str

    [reveal_type(x) for _ in range(1) if x is not None]  # revealed: str
```

### Narrowing constraints introduced in the outer scope

```py
g: str | None = "a"

class A:
    x: str | None = None

a = A()

l: list[str | None] = [None]

def f(x: str | None):
    if x is not None:
        def _():
            # If there is a possibility that `x` may be rewritten after this function definition,
            # the constraint `x is not None` outside the function is no longer be applicable for narrowing.
            reveal_type(x)  # revealed: str | None

        class C:
            reveal_type(x)  # revealed: str

        [reveal_type(x) for _ in range(1)]  # revealed: str

    # When there is a reassignment, any narrowing constraints on the place are invalidated in lazy scopes.
    x = None

def f(x: str | None):
    def _():
        if x is not None:
            def closure():
                reveal_type(x)  # revealed: str | None
    x = None

def f(x: str | None):
    def _(x: str | None):
        if x is not None:
            def closure():
                reveal_type(x)  # revealed: str
    x = None

def f(x: str | None):
    class C:
        def _():
            if x is not None:
                def closure():
                    reveal_type(x)  # revealed: str
        x = None  # This assignment is not visible in the inner lazy scope, so narrowing is still valid.
```

If a variable defined in a private scope is never reassigned, narrowing remains in effect in the
inner lazy scope.

```py
def f(const: str | None):
    if const is not None:
        def _():
            # The `const is not None` narrowing constraint is still valid since `const` has not been reassigned
            reveal_type(const)  # revealed: str

        class C2:
            reveal_type(const)  # revealed: str

        [reveal_type(const) for _ in range(1)]  # revealed: str

def f(const: str | None):
    def _():
        if const is not None:
            def closure():
                reveal_type(const)  # revealed: str
```

And even if there is an attribute or subscript assignment to the variable, narrowing of the variable
is still valid in the inner lazy scope.

```py
def f(l: list[str | None] | None):
    if l is not None:
        def _():
            reveal_type(l)  # revealed: list[str | None]
        l[0] = None

def f(a: A):
    if a:
        def _():
            reveal_type(a)  # revealed: A & ~AlwaysFalsy
    a.x = None
```

The opposite is not true, that is, if a root expression is reassigned, narrowing on the member are
no longer valid in the inner lazy scope.

```py
def f(l: list[str | None]):
    if l[0] is not None:
        def _():
            reveal_type(l[0])  # revealed: str | None
        l = [None]

def f(l: list[str | None]):
    l[0] = "a"
    def _():
        reveal_type(l[0])  # revealed: str | None
    l = [None]

def f(l: list[str | None]):
    l[0] = "a"
    def _():
        l: list[str | None] = [None]
        def _():
            reveal_type(l[0])  # revealed: str | None

    def _():
        def _():
            reveal_type(l[0])  # revealed: str | None
        l: list[str | None] = [None]

def f(a: A):
    if a.x is not None:
        def _():
            reveal_type(a.x)  # revealed: str | None
    a = A()

def f(a: A):
    a.x = "a"
    def _():
        reveal_type(a.x)  # revealed: str | None
    a = A()
```

Narrowing is also invalidated if a `nonlocal` declaration is made within a lazy scope.

```py
def f(non_local: str | None):
    if non_local is not None:
        def _():
            nonlocal non_local
            non_local = None

        def _():
            reveal_type(non_local)  # revealed: str | None

def f(non_local: str | None):
    def _():
        nonlocal non_local
        non_local = None
    if non_local is not None:
        def _():
            reveal_type(non_local)  # revealed: str | None
```

Nonlocal writes do not invalidate snapshots for other names. Eager scopes retain their narrowing
even when a lazy scope can write the same name.

```py
def f(value: str | None, other: str | None):
    if value is not None and other is not None:
        class Eager:
            reveal_type(value)  # revealed: str

        def read():
            reveal_type(value)  # revealed: str | None
            reveal_type(other)  # revealed: str

        def write():
            nonlocal value
            value = None
```

A nonlocal write in an earlier, unrelated scope does not invalidate a later snapshot of the same
name.

```py
def earlier(value: str | None):
    def write():
        nonlocal value
        value = None

def later(value: str | None):
    if value is not None:
        def read():
            reveal_type(value)  # revealed: str
```

The same goes for public variables, attributes, and subscripts, because it is difficult to track all
of their changes.

```py
def f():
    if g is not None:
        def _():
            reveal_type(g)  # revealed: str | None

        class D:
            reveal_type(g)  # revealed: str

        [reveal_type(g) for _ in range(1)]  # revealed: str

    if a.x is not None:
        def _():
            # Lazy nested scope narrowing is not performed on attributes/subscripts because it's difficult to track their changes.
            reveal_type(a.x)  # revealed: str | None

        class D:
            reveal_type(a.x)  # revealed: str

        [reveal_type(a.x) for _ in range(1)]  # revealed: str

    if l[0] is not None:
        def _():
            reveal_type(l[0])  # revealed: str | None

        class D:
            reveal_type(l[0])  # revealed: str

        [reveal_type(l[0]) for _ in range(1)]  # revealed: str
```

### Narrowing constraints introduced in multiple scopes

```py
from typing import Literal

g: str | Literal[1] | None = "a"

class A:
    x: str | Literal[1] | None = None

a = A()

l: list[str | Literal[1] | None] = [None]

def f(x: str | Literal[1] | None):
    class C:
        # If we try to access a variable in a class before it has been defined,
        # the lookup will fall back to global.
        # error: [unresolved-reference]
        if x is not None:
            def _():
                if x != 1:
                    reveal_type(x)  # revealed: str | None

            class D:
                if x != 1:
                    reveal_type(x)  # revealed: str

            [reveal_type(x) for _ in range(1) if x != 1]  # revealed: str

        x = None

    def _():
        # No narrowing is performed on unresolved references.
        # error: [unresolved-reference]
        if x is not None:
            def _():
                if x != 1:
                    reveal_type(x)  # revealed: None
        x = None

def f(const: str | Literal[1] | None):
    class C:
        if const is not None:
            def _():
                if const != 1:
                    reveal_type(const)  # revealed: str

            class D:
                if const != 1:
                    reveal_type(const)  # revealed: str

            [reveal_type(const) for _ in range(1) if const != 1]  # revealed: str

    def _():
        if const is not None:
            def _():
                if const != 1:
                    reveal_type(const)  # revealed: str

def f():
    class C:
        if g is not None:
            def _():
                if g != 1:
                    reveal_type(g)  # revealed: str | None

            class D:
                if g != 1:
                    reveal_type(g)  # revealed: str

        if a.x is not None:
            def _():
                if a.x != 1:
                    reveal_type(a.x)  # revealed: str | None

            class D:
                if a.x != 1:
                    reveal_type(a.x)  # revealed: str

        if l[0] is not None:
            def _():
                if l[0] != 1:
                    reveal_type(l[0])  # revealed: str | None

            class D:
                if l[0] != 1:
                    reveal_type(l[0])  # revealed: str
```

### Narrowing constraints with bindings in class scope, and nested scopes

```py
from typing import Literal

g: str | Literal[1] | None = "a"

def f(flag: bool):
    class C:
        (g := None) if flag else (g := None)
        # `g` is always bound here, so narrowing checks don't apply to nested scopes
        if g is not None:
            class F:
                reveal_type(g)  # revealed: str | Literal[1] | None

    class C:
        # this conditional binding leaves "unbound" visible, so following narrowing checks apply
        None if flag else (g := None)

        if g is not None:
            class F:
                reveal_type(g)  # revealed: str | Literal[1]

            # This class variable is not visible from the nested class scope.
            g = None

            # This additional constraint is not relevant to nested scopes, since it only applies to
            # a binding of `g` that they cannot see:
            if g is None:
                class E:
                    reveal_type(g)  # revealed: str | Literal[1]
```

### Deleting a class-local name

A constraint on an outer member survives a class-local binding and deletion unless a write through
the class-local name may have changed it. After deletion, a new constraint on the outer member is
visible to nested classes when both scopes resolve the name to the same outer binding, but a
constraint on a later class-local value is not. In `conditional`, the class body resolves `value` to
the module, while the nested class resolves it to the function.

```py
class Box:
    item: int | str

value = Box()

class Outer:
    if isinstance(value.item, int):
        value = Box()
        value.item = "local"
        del value

        class AfterDeletion:
            reveal_type(value.item)  # revealed: int | Literal["local"]

    value = Box()
    del value
    if isinstance(value.item, str):
        class Inner:
            reveal_type(value.item)  # revealed: str

        value = Box()
        if isinstance(value.item, int):
            class AfterRebinding:
                reveal_type(value.item)  # revealed: str

def conditional(flag: bool):
    value = Box()

    class Outer:
        if flag:
            value = Box()
            del value
        if isinstance(value.item, str):
            class Inner:
                reveal_type(value.item)  # revealed: int | str
```

### Member assignments after a conditional class-local binding

An assignment can update the outer object when the class-local name is not bound or when the
class-local and outer names refer to the same object. Nested classes must account for both
possibilities. Here ty conservatively allows the names to refer to the same object even after a
`Box()` call.

```py
class Box:
    item: int | str = "initial"

flag: bool = bool(input())
conditional_value = Box()
bound_value = Box()
unbound_value = Box()
false_value = Box()
written_value = Box()

class Conditional:
    if isinstance(conditional_value.item, str):
        if flag:
            conditional_value = Box()
        conditional_value.item = 1

        class Inner:
            reveal_type(conditional_value.item)  # revealed: str | Literal[1]
            # error: [unresolved-attribute]
            conditional_value.item.upper()

class Bound:
    if isinstance(bound_value.item, str):
        bound_value = Box()
        bound_value.item = 1

        class Inner:
            reveal_type(bound_value.item)  # revealed: str | Literal[1]

class Unbound:
    if isinstance(unbound_value.item, str):
        unbound_value.item = 1

        class Inner:
            reveal_type(unbound_value.item)  # revealed: Literal[1]

class OnlyOuter:
    if isinstance(false_value.item, str):
        if False:
            false_value = Box()
        false_value.item = 1

        class Inner:
            reveal_type(false_value.item)  # revealed: Literal[1]

class PreviousWrite:
    written_value.item = "initial"
    if flag:
        written_value = Box()
    written_value.item = 1

    class Inner:
        reveal_type(written_value.item)  # revealed: Literal["initial", 1]
```

The same applies to a subscript of a `TypedDict` and when deletion makes a class-local name unbound.

```py
from typing import TypedDict

class Data(TypedDict):
    item: int | str

def get_data() -> Data:
    return {"item": "outer"}

data = get_data()
deleted_value = Box()

class ConditionalSubscript:
    if isinstance(data["item"], str):
        if flag:
            data = Data(item="local")
        data["item"] = 1

        class Inner:
            reveal_type(data["item"])  # revealed: str | Literal[1]

class ConditionalDeletion:
    if isinstance(deleted_value.item, str):
        deleted_value = Box()
        if flag:
            del deleted_value
        deleted_value.item = 1

        class Inner:
            reveal_type(deleted_value.item)  # revealed: str | Literal[1]
```

### Replacing an ancestor of a narrowed member

Replacing an attribute can invalidate a constraint on one of its descendants, including when the
attribute is accessed through a possibly unbound class-local name.

```py
class Child:
    def __init__(self, item: int | str):
        self.item = item

class Parent:
    def __init__(self):
        self.child = Child("initial")

flag: bool = bool(input())
parent = Parent()
local_parent = Parent()

class Outer:
    if isinstance(parent.child.item, str):
        if flag:
            parent = Parent()
        parent.child = Child(1)

        class Inner:
            reveal_type(parent.child.item)  # revealed: int | str

class Local:
    if isinstance(local_parent.child.item, str):
        local_parent = Parent()
        local_parent.child = Child(1)

        class Inner:
            reveal_type(local_parent.child.item)  # revealed: int | str
```

### Deleting a member

Deleting an attribute through a name that is unbound in the class body can change the outer object.
In these examples, deleting an instance attribute exposes a class attribute, so the earlier
narrowing may no longer apply.

```py
class Box:
    item: int | str = 1

    def __init__(self, item: int | str = "initial"):
        self.item = item

flag: bool = bool(input())
value = Box()

class Outer:
    if isinstance(value.item, str):
        if flag:
            value = Box()
        del value.item

        class Inner:
            reveal_type(value.item)  # revealed: int | str

class Parent:
    child: Box = Box(1)

    def __init__(self):
        self.child = Box()

parent = Parent()

class DeleteAncestor:
    if isinstance(parent.child.item, str):
        if flag:
            parent = Parent()
        del parent.child

        class Inner:
            reveal_type(parent.child.item)  # revealed: int | str
```

Member deletion also invalidates descendant narrowing in a function.

```py
def delete_in_function(parent: Parent):
    if isinstance(parent.child.item, str):
        del parent.child
        reveal_type(parent.child.item)  # revealed: int | str
```

### Class-local names that alias an enclosing object

A class-local name and the name visible in a nested class may refer to the same object. Writes and
deletions through the class-local name can therefore change the value seen by the nested class.

```py
from typing import TypedDict

class Box:
    item: int | str = 1

    def __init__(self, item: int | str):
        self.item = item

class Data(TypedDict):
    item: int | str

value = Box("outer")
data: Data = {"item": "outer"}

class Assignment:
    if isinstance(value.item, str):
        value = value
        value.item = 1

        class Inner:
            reveal_type(value.item)  # revealed: str | Literal[1]
            # error: [unresolved-attribute]
            value.item.upper()

class SubscriptAssignment:
    if isinstance(data["item"], str):
        data = data
        data["item"] = 1

        class Inner:
            reveal_type(data["item"])  # revealed: str | Literal[1]

class Deletion:
    if isinstance(value.item, str):
        value = value
        del value.item

        class Inner:
            reveal_type(value.item)  # revealed: int | str
```

### Class-body and nested-class names can resolve to different scopes

When a class binds a name, an earlier unbound use in that class falls back to the module. A nested
class can instead resolve the name to an enclosing function. Constraints on the module value do not
narrow the function's value.

```py
class Box:
    item: int | str

    def __init__(self, item: int | str):
        self.item = item

value = Box("module")

def local(value: Box) -> None:
    class Outer:
        if isinstance(value.item, str):
            value = value
            value.item = "changed"

            class Inner:
                reveal_type(value.item)  # revealed: int | str
                # error: [unresolved-attribute]
                value.item.upper()

            items = [value.item for _ in range(1)]
            reveal_type(items)  # revealed: list[int | str]

            def later() -> None:
                reveal_type(value.item)  # revealed: int | str

def global_declaration(value: Box) -> None:
    class Outer:
        global value
        value.item = 1

        class Inner:
            reveal_type(value.item)  # revealed: int | str
            # error: [unresolved-attribute]
            value.item.bit_length()

def intermediate_class(value: Box) -> None:
    class Outer:
        if isinstance(value.item, str):
            value = value

            class Middle:
                if isinstance(value.item, int):
                    class Inner:
                        reveal_type(value.item)  # revealed: int

def nonlocal_declaration(value: Box) -> None:
    class Outer:
        nonlocal value
        if isinstance(value.item, str):
            class Inner:
                reveal_type(value.item)  # revealed: str
```

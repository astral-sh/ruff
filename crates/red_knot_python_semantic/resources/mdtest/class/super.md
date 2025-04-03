# Super

Python defines the terms *bound super object* and *unbound super object*.

An **unbound super object** is created when `super` is called with only one argument. (e.g.
`super(A)`). This object may later be bound using the `super.__get__` method. However, this form is
rarely used in practice.

A **bound super object** is created either by calling `super(pivot_class, owner)` or by using the
implicit form `super()`, where both the pivot class and the owner are inferred. This is the most
common usage.

## Basic Usage

### Explicit Bound Super Object

`super(pivot_class, owner)` performs attribute lookup along the MRO, starting immediately after the
specified pivot class.

```py
class A:
    def a(self): ...
    aa: int = 1

class B(A):
    def b(self): ...
    bb: int = 2

class C(B):
    def c(self): ...
    cc: int = 3

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[B], Literal[A], Literal[object]]

super(C, C()).a
super(C, C()).b
# error: [unresolved-attribute] "Type `<super: Literal[C], C>` has no attribute `c`"
super(C, C()).c

super(B, C()).a
# error: [unresolved-attribute] "Type `<super: Literal[B], C>` has no attribute `b`"
super(B, C()).b
# error: [unresolved-attribute] "Type `<super: Literal[B], C>` has no attribute `c`"
super(B, C()).c

# error: [unresolved-attribute] "Type `<super: Literal[A], C>` has no attribute `a`"
super(A, C()).a
# error: [unresolved-attribute] "Type `<super: Literal[A], C>` has no attribute `b`"
super(A, C()).b
# error: [unresolved-attribute] "Type `<super: Literal[A], C>` has no attribute `c`"
super(A, C()).c

reveal_type(super(C, C()).a)  # revealed: <bound method `a` of `C`>
reveal_type(super(C, C()).b)  # revealed: <bound method `b` of `C`>
reveal_type(super(C, C()).aa)  # revealed: int
reveal_type(super(C, C()).bb)  # revealed: int
```

### Implicit Bound Super Object

The implicit form `super()` is same as `super(__class__, <first argument>)`. The `__class__` refers
to the class that contains the function where `super()` is used. The first argument refers to the
current method’s first parameter (typically `self` or `cls`).

```py
from __future__ import annotations
from knot_extensions import TypeOf

class A:
    def __init__(self, a: int): ...
    @classmethod
    def f(cls): ...

# TODO: This should work the same even without explicitly annotating `self`.
class B(A):
    def __init__(self: B, a: int):
        reveal_type(super())  # revealed: <super: Literal[B], B>
        reveal_type(super().__init__)  # revealed: <bound method `__init__` of `B`>
        super().__init__(a)

    @classmethod
    def f(cls: TypeOf[B]):
        reveal_type(super())  # revealed: <super: Literal[B], Literal[B]>
        reveal_type(super().f)  # revealed: <bound method `f` of `Literal[B]`>
        super().f()
```

### Plain `super` Instance

In certain cases, the inferred type may be as a plain `super` instance. This typically happens when
the proper MRO cannot be determined.

A plain `super` instance does not allow attribute access, since its MRO is meaningless.

#### Unbound Super Object

```py
class A:
    a: int = 1

class B(A):
    b: int = 2

reveal_type(super(B))  # revealed: super
super(B).a  # error: [unresolved-attribute] "Type `super` has no attribute `a`"
super(B).b  # error: [unresolved-attribute] "Type `super` has no attribute `b`"
```

#### Dynamic Types

```py
def f(x):
    reveal_type(x)  # revealed: Unknown
    reveal_type(super(x, x))  # revealed: super
    reveal_type(super(int, x))  # revealed: super
    super(x, x).x  # error: [unresolved-attribute] "Type `super` has no attribute `x`"
```

## Implicit Bound Super in Complex Structure

```py
from __future__ import annotations

# TODO: This should work the same even without explicitly annotating `self`.
class A:
    def test(self: A):
        reveal_type(super())  # revealed: <super: Literal[A], A>

    class B:
        def test(self: A.B):
            reveal_type(super())  # revealed: <super: Literal[B], B>

            class C(A.B):
                def test(self: C):
                    reveal_type(super())  # revealed: <super: Literal[C], C>

            def inner(t: C):
                reveal_type(super())  # revealed: <super: Literal[B], C>
            lambda x: reveal_type(super())  # revealed: super
```

## Built-ins and Literals

```py
reveal_type(super(bool, True))  # revealed: <super: Literal[bool], bool>
reveal_type(super(bool, bool()))  # revealed: <super: Literal[bool], bool>
reveal_type(super(int, bool()))  # revealed: <super: Literal[int], bool>
```

## Descriptor Behavior with Super

Accessing attributes through `super` still invokes descriptor protocol. However, the behavior can
differ depending on whether the second argument to super is a class or an instance.

```py
class A:
    def a1(self): ...
    @classmethod
    def a2(cls): ...

class B(A): ...

# A.__dict__["a1"].__get__(B(), B)
reveal_type(super(B, B()).a1)  # revealed: <bound method `a1` of `B`>
# A.__dict__["a2"].__get__(B(), B)
reveal_type(super(B, B()).a2)  # revealed: <bound method `a2` of `Literal[B]`>

# A.__dict__["a1"].__get__(None, B)
reveal_type(super(B, B).a1)  # revealed: Literal[a1]
# A.__dict__["a2"].__get__(None, B)
reveal_type(super(B, B).a2)  # revealed: <bound method `a2` of `Literal[B]`>
```

## Invalid Usages

### Unresolvable `super()` Calls

If an appropriate class and argument cannot be found, a runtime error will occur. TODO: Proper
diagnostics should be provided in these cases.

```py
from __future__ import annotations

reveal_type(super())  # revealed: Unknown

def f():
    reveal_type(super())  # revealed: Unknown

# No first argument in its scope
# TODO: This should work the same even without explicitly annotating `self`.
class A:
    def f(self: A):
        def g():
            reveal_type(super())  # revealed: Unknown
        lambda: reveal_type(super())  # revealed: Unknown

        (reveal_type(super()) for _ in range(10))  # revealed: Unknown
```

### Failing Condition Checks

TODO: Proper diagnostics should be provided in these cases.

```py
# does not satisfy `isinstance(str(), int)`
reveal_type(super(int, str()))  # revealed: Unknown
# does not satisfy `issubclass(str, int)`
reveal_type(super(int, str))  # revealed: Unknown

class A: ...
class B(A): ...

reveal_type(super(B, A()))  # revealed: Unknown
reveal_type(super(B, object()))  # revealed: Unknown

reveal_type(super(B, A))  # revealed: Unknown
reveal_type(super(B, object))  # revealed: Unknown
```

### Instance Member Access via `super`

```py
from __future__ import annotations

class A:
    def __init__(self, a: int):
        self.a = a

# TODO: This should work the same even without explicitly annotating `self`.
class B(A):
    def __init__(self: B, a: int):
        super().__init__(a)
        # error: [unresolved-attribute] "Type `<super: Literal[B], B>` has no attribute `a`"
        super().a
```

### Dunder Method Resolution

Dunder methods defined in the `owner` (from `super(pivot_class, owner)`) should not affect the super
object itself. In other words, `super` should not be treated as if it inherits attributes of the
`owner`.

```py
class A:
    def __getitem__(self, key: int) -> int:
        return 42

class B(A): ...

reveal_type(A()[0])  # revealed: int
reveal_type(super(B, B()).__getitem__)  # revealed: <bound method `__getitem__` of `B`>
# error: [non-subscriptable] "Cannot subscript object of type `<super: Literal[B], B>` with no `__getitem__` method"
super(B, B())[0]
```

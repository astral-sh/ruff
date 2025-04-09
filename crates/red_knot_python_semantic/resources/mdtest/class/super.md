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

class A:
    def __init__(self, a: int): ...
    @classmethod
    def f(cls): ...

class B(A):
    def __init__(self, a: int):
        reveal_type(super())  # revealed: <super: Literal[B], Unknown>
        # TODO: Once `Self` is supported, this should be `<bound method `__init__` of `B`>`
        reveal_type(super().__init__)  # revealed: Unknown
        super().__init__(a)

    @classmethod
    def f(cls):
        reveal_type(super())  # revealed: <super: Literal[B], Unknown>
        # TODO: Once `Self` is supported, this should be `<bound method `f` of `Literal[B]`>`
        reveal_type(super().f)  # revealed: Unknown
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
```

## Dynamic Types

```py
class A:
    a: int = 1

def f(x):
    reveal_type(x)  # revealed: Unknown
    reveal_type(super(x, x))  # revealed: <super: Unknown, Unknown>
    reveal_type(super(int, x))  # revealed: <super: Literal[int], Unknown>
    reveal_type(super(x, int()))  # revealed: <super: Unknown, int>

    reveal_type(super(x, x).a)  # revealed: Unknown
    reveal_type(super(A, x).a)  # revealed: Unknown
    reveal_type(super(x, A()).a)  # revealed: int
```

## Implicit Bound Super in Complex Structure

```py
from __future__ import annotations

class A:
    def test(self):
        reveal_type(super())  # revealed: <super: Literal[A], Unknown>

    class B:
        def test(self):
            reveal_type(super())  # revealed: <super: Literal[B], Unknown>

            class C(A.B):
                def test(self):
                    reveal_type(super())  # revealed: <super: Literal[C], Unknown>

            def inner(t: C):
                reveal_type(super())  # revealed: <super: Literal[B], C>
            lambda x: reveal_type(super())  # revealed: <super: Literal[B], Unknown>
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

## Union of supers

```py
class A:
    x = 1
    y: int = 1

    a: str = "a only"

class B(A): ...

class C:
    x = 2
    y: int | str = "test"

class D(C): ...

def f(flag: bool):
    s = super(B, B()) if flag else super(D, D())

    reveal_type(s)  # revealed: <super: Literal[B], B> | <super: Literal[D], D>

    reveal_type(s.x)  # revealed: Unknown | Literal[1, 2]
    reveal_type(s.y)  # revealed: int | str

    # error: [possibly-unbound-attribute] "Attribute `a` on type `<super: Literal[B], B> | <super: Literal[D], D>` is possibly unbound"
    reveal_type(s.a)  # revealed: str
```

## Invalid Usages

### Unresolvable `super()` Calls

If an appropriate class and argument cannot be found, a runtime error will occur.

```py
from __future__ import annotations

# error: [unavailable-implicit-super-arguments] "Implicit arguments for `super()` are not available in this context"
reveal_type(super())  # revealed: Unknown

def f():
    # error: [unavailable-implicit-super-arguments] "Implicit arguments for `super()` are not available in this context"
    super()

# No first argument in its scope
class A:
    # error: [unavailable-implicit-super-arguments] "Implicit arguments for `super()` are not available in this context"
    s = super()

    def f(self):
        def g():
            # error: [unavailable-implicit-super-arguments] "Implicit arguments for `super()` are not available in this context"
            super()
        # error: [unavailable-implicit-super-arguments] "Implicit arguments for `super()` are not available in this context"
        lambda: super()

        # error: [unavailable-implicit-super-arguments] "Implicit arguments for `super()` are not available in this context"
        (super() for _ in range(10))

    @staticmethod
    def h():
        # error: [unavailable-implicit-super-arguments] "Implicit arguments for `super()` are not available in this context"
        super()
```

### Failing Condition Checks

```py
# does not satisfy `isinstance(str(), int)`
# error: [invalid-super-argument] "Second argument `Literal[""]` is not an instance or subclass of `Literal[int]` in `super(Literal[int], Literal[""])` call"
reveal_type(super(int, str()))  # revealed: Unknown

# does not satisfy `issubclass(str, int)`
# error: [invalid-super-argument] "Second argument `Literal[str]` is not an instance or subclass of `Literal[int]` in `super(Literal[int], Literal[str])` call"
reveal_type(super(int, str))  # revealed: Unknown

class A: ...
class B(A): ...

# error: [invalid-super-argument] "Second argument `A` is not an instance or subclass of `Literal[B]` in `super(Literal[B], A)` call"
reveal_type(super(B, A()))  # revealed: Unknown
# error: [invalid-super-argument] "Second argument `object` is not an instance or subclass of `Literal[B]` in `super(Literal[B], object)` call"
reveal_type(super(B, object()))  # revealed: Unknown

# error: [invalid-super-argument] "Second argument `Literal[A]` is not an instance or subclass of `Literal[B]` in `super(Literal[B], Literal[A])` call"
reveal_type(super(B, A))  # revealed: Unknown
# error: [invalid-super-argument] "Second argument `Literal[object]` is not an instance or subclass of `Literal[B]` in `super(Literal[B], Literal[object])` call"
reveal_type(super(B, object))  # revealed: Unknown
```

### Instance Member Access via `super`

Accessing instance members through `super()` is not allowed.

```py
from __future__ import annotations

class A:
    def __init__(self, a: int):
        self.a = a

class B(A):
    def __init__(self, a: int):
        super().__init__(a)
        # TODO: Once `Self` is supported, this should raise `unresolved-attribute` error
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

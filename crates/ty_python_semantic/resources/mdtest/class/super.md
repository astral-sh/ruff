# Super

Python defines the terms _bound super object_ and _unbound super object_.

An **unbound super object** is created when `super` is called with only one argument. (e.g.
`super(A)`). This object may later be bound using the `super.__get__` method. However, this form is
rarely used in practice.

A **bound super object** is created either by calling `super(pivot_class, owner)` or by using the
implicit form `super()`, where both the pivot class and the owner are inferred. This is the most
common usage.

## Basic Usage

### Explicit Super Object

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

reveal_type(C.__mro__)  # revealed: tuple[<class 'C'>, <class 'B'>, <class 'A'>, <class 'object'>]

super(C, C()).a
super(C, C()).b
# error: [unresolved-attribute] "Type `<super: <class 'C'>, C>` has no attribute `c`"
super(C, C()).c

super(B, C()).a
# error: [unresolved-attribute] "Type `<super: <class 'B'>, C>` has no attribute `b`"
super(B, C()).b
# error: [unresolved-attribute] "Type `<super: <class 'B'>, C>` has no attribute `c`"
super(B, C()).c

# error: [unresolved-attribute] "Type `<super: <class 'A'>, C>` has no attribute `a`"
super(A, C()).a
# error: [unresolved-attribute] "Type `<super: <class 'A'>, C>` has no attribute `b`"
super(A, C()).b
# error: [unresolved-attribute] "Type `<super: <class 'A'>, C>` has no attribute `c`"
super(A, C()).c

reveal_type(super(C, C()).a)  # revealed: bound method C.a() -> Unknown
reveal_type(super(C, C()).b)  # revealed: bound method C.b() -> Unknown
reveal_type(super(C, C()).aa)  # revealed: int
reveal_type(super(C, C()).bb)  # revealed: int
```

### Implicit Super Object

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
        # TODO: Once `Self` is supported, this should be `<super: <class 'B'>, B>`
        reveal_type(super())  # revealed: <super: <class 'B'>, Unknown>
        super().__init__(a)

    @classmethod
    def f(cls):
        # TODO: Once `Self` is supported, this should be `<super: <class 'B'>, <class 'B'>>`
        reveal_type(super())  # revealed: <super: <class 'B'>, Unknown>
        super().f()

super(B, B(42)).__init__(42)
super(B, B).f()
```

### Unbound Super Object

Calling `super(cls)` without a second argument returns an _unbound super object_. This is treated as
a plain `super` instance and does not support name lookup via the MRO.

```py
class A:
    a: int = 42

class B(A): ...

reveal_type(super(B))  # revealed: super

# error: [unresolved-attribute] "Type `super` has no attribute `a`"
super(B).a
```

## Attribute Assignment

`super()` objects do not allow attribute assignment — even if the attribute is resolved
successfully.

```py
class A:
    a: int = 3

class B(A): ...

reveal_type(super(B, B()).a)  # revealed: int
# error: [invalid-assignment] "Cannot assign to attribute `a` on type `<super: <class 'B'>, B>`"
super(B, B()).a = 3
# error: [invalid-assignment] "Cannot assign to attribute `a` on type `super`"
super(B).a = 5
```

## Dynamic Types

If any of the arguments is dynamic, we cannot determine the MRO to traverse. When accessing a
member, it should effectively behave like a dynamic type.

```py
class A:
    a: int = 1

def f(x):
    reveal_type(x)  # revealed: Unknown

    reveal_type(super(x, x))  # revealed: <super: Unknown, Unknown>
    reveal_type(super(A, x))  # revealed: <super: <class 'A'>, Unknown>
    reveal_type(super(x, A()))  # revealed: <super: Unknown, A>

    reveal_type(super(x, x).a)  # revealed: Unknown
    reveal_type(super(A, x).a)  # revealed: Unknown
    reveal_type(super(x, A()).a)  # revealed: Unknown
```

## Implicit `super()` in Complex Structure

```py
from __future__ import annotations

class A:
    def test(self):
        reveal_type(super())  # revealed: <super: <class 'A'>, Unknown>

    class B:
        def test(self):
            reveal_type(super())  # revealed: <super: <class 'B'>, Unknown>

            class C(A.B):
                def test(self):
                    reveal_type(super())  # revealed: <super: <class 'C'>, Unknown>

            def inner(t: C):
                reveal_type(super())  # revealed: <super: <class 'B'>, C>
            lambda x: reveal_type(super())  # revealed: <super: <class 'B'>, Unknown>
```

## Built-ins and Literals

```py
reveal_type(super(bool, True))  # revealed: <super: <class 'bool'>, bool>
reveal_type(super(bool, bool()))  # revealed: <super: <class 'bool'>, bool>
reveal_type(super(int, bool()))  # revealed: <super: <class 'int'>, bool>
reveal_type(super(int, 3))  # revealed: <super: <class 'int'>, int>
reveal_type(super(str, ""))  # revealed: <super: <class 'str'>, str>
```

## Descriptor Behavior with Super

Accessing attributes through `super` still invokes descriptor protocol. However, the behavior can
differ depending on whether the second argument to `super` is a class or an instance.

```py
class A:
    def a1(self): ...
    @classmethod
    def a2(cls): ...

class B(A): ...

# A.__dict__["a1"].__get__(B(), B)
reveal_type(super(B, B()).a1)  # revealed: bound method B.a1() -> Unknown
# A.__dict__["a2"].__get__(B(), B)
reveal_type(super(B, B()).a2)  # revealed: bound method type[B].a2() -> Unknown

# A.__dict__["a1"].__get__(None, B)
reveal_type(super(B, B).a1)  # revealed: def a1(self) -> Unknown
# A.__dict__["a2"].__get__(None, B)
reveal_type(super(B, B).a2)  # revealed: bound method <class 'B'>.a2() -> Unknown
```

## Union of Supers

When the owner is a union type, `super()` is built separately for each branch, and the resulting
super objects are combined into a union.

```py
class A: ...

class B:
    b: int = 42

class C(A, B): ...
class D(B, A): ...

def f(x: C | D):
    reveal_type(C.__mro__)  # revealed: tuple[<class 'C'>, <class 'A'>, <class 'B'>, <class 'object'>]
    reveal_type(D.__mro__)  # revealed: tuple[<class 'D'>, <class 'B'>, <class 'A'>, <class 'object'>]

    s = super(A, x)
    reveal_type(s)  # revealed: <super: <class 'A'>, C> | <super: <class 'A'>, D>

    # error: [possibly-unbound-attribute] "Attribute `b` on type `<super: <class 'A'>, C> | <super: <class 'A'>, D>` is possibly unbound"
    s.b

def f(flag: bool):
    x = str() if flag else str("hello")
    reveal_type(x)  # revealed: Literal["", "hello"]
    reveal_type(super(str, x))  # revealed: <super: <class 'str'>, str>

def f(x: int | str):
    # error: [invalid-super-argument] "`str` is not an instance or subclass of `<class 'int'>` in `super(<class 'int'>, str)` call"
    super(int, x)
```

Even when `super()` is constructed separately for each branch of a union, it should behave correctly
in all cases.

```py
def f(flag: bool):
    if flag:
        class A:
            x = 1
            y: int = 1

            a: str = "hello"

        class B(A): ...
        s = super(B, B())
    else:
        class C:
            x = 2
            y: int | str = "test"

        class D(C): ...
        s = super(D, D())

    reveal_type(s)  # revealed: <super: <class 'B'>, B> | <super: <class 'D'>, D>

    reveal_type(s.x)  # revealed: Unknown | Literal[1, 2]
    reveal_type(s.y)  # revealed: int | str

    # error: [possibly-unbound-attribute] "Attribute `a` on type `<super: <class 'B'>, B> | <super: <class 'D'>, D>` is possibly unbound"
    reveal_type(s.a)  # revealed: str
```

## Supers with Generic Classes

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import TypeOf, static_assert, is_subtype_of

class A[T]:
    def f(self, a: T) -> T:
        return a

class B[T](A[T]):
    def f(self, b: T) -> T:
        return super().f(b)
```

## Invalid Usages

### Unresolvable `super()` Calls

If an appropriate class and argument cannot be found, a runtime error will occur.

```py
from __future__ import annotations

# error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
reveal_type(super())  # revealed: Unknown

def f():
    # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
    super()

# No first argument in its scope
class A:
    # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
    s = super()

    def f(self):
        def g():
            # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
            super()
        # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
        lambda: super()

        # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
        (super() for _ in range(10))

    @staticmethod
    def h():
        # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
        super()
```

### Failing Condition Checks

```toml
[environment]
python-version = "3.12"
```

`super()` requires its first argument to be a valid class, and its second argument to be either an
instance or a subclass of the first. If either condition is violated, a `TypeError` is raised at
runtime.

```py
def f(x: int):
    # error: [invalid-super-argument] "`int` is not a valid class"
    super(x, x)

    type IntAlias = int
    # error: [invalid-super-argument] "`typing.TypeAliasType` is not a valid class"
    super(IntAlias, 0)

# error: [invalid-super-argument] "`Literal[""]` is not an instance or subclass of `<class 'int'>` in `super(<class 'int'>, Literal[""])` call"
# revealed: Unknown
reveal_type(super(int, str()))

# error: [invalid-super-argument] "`<class 'str'>` is not an instance or subclass of `<class 'int'>` in `super(<class 'int'>, <class 'str'>)` call"
# revealed: Unknown
reveal_type(super(int, str))

class A: ...
class B(A): ...

# error: [invalid-super-argument] "`A` is not an instance or subclass of `<class 'B'>` in `super(<class 'B'>, A)` call"
# revealed: Unknown
reveal_type(super(B, A()))

# error: [invalid-super-argument] "`object` is not an instance or subclass of `<class 'B'>` in `super(<class 'B'>, object)` call"
# revealed: Unknown
reveal_type(super(B, object()))

# error: [invalid-super-argument] "`<class 'A'>` is not an instance or subclass of `<class 'B'>` in `super(<class 'B'>, <class 'A'>)` call"
# revealed: Unknown
reveal_type(super(B, A))

# error: [invalid-super-argument] "`<class 'object'>` is not an instance or subclass of `<class 'B'>` in `super(<class 'B'>, <class 'object'>)` call"
# revealed: Unknown
reveal_type(super(B, object))

super(object, object()).__class__
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

# error: [unresolved-attribute] "Type `<super: <class 'B'>, B>` has no attribute `a`"
super(B, B(42)).a
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
reveal_type(super(B, B()).__getitem__)  # revealed: bound method B.__getitem__(key: int) -> int
# error: [non-subscriptable] "Cannot subscript object of type `<super: <class 'B'>, B>` with no `__getitem__` method"
super(B, B())[0]
```

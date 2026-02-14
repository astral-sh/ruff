# `__slots__`

## Basic slot access

```py
class A:
    __slots__ = ("foo", "bar")

    def __init__(self, foo: int, bar: str):
        self.foo = foo
        self.bar = bar

a = A(1, "zip")
a.foo = 2
a.bar = "woo"
a.baz = 3  # error: [unresolved-attribute]
```

## Accessing slot-declared but uninitialized attributes

A slot declaration is analogous to a bare annotation like `x: int` in a class body: the attribute is
considered declared and accessible on instances, even if it was never explicitly initialized.

```py
class A:
    __slots__ = ("x",)

a = A()
reveal_type(a.x)  # revealed: Unknown
a.x = 1
```

## Accessing undefined attributes

```py
class A:
    __slots__ = ("x",)

a = A()
a.y = 1  # error: [unresolved-attribute]
```

## Empty slots

```py
class A:
    __slots__ = ()

a = A()
a.x = 1  # error: [unresolved-attribute]
```

## Single character string

```py
class A:
    __slots__ = "x"

a = A()
a.x = 1
a.y = 2  # error: [unresolved-attribute]
```

## Multi-character string

Python treats `__slots__ = "xyz"` as a single slot named `"xyz"`, not three individual character
slots.

```py
class A:
    __slots__ = "xyz"

a = A()
a.xyz = 1
a.x = 2  # error: [unresolved-attribute]
a.y = 3  # error: [unresolved-attribute]
a.z = 4  # error: [unresolved-attribute]
a.q = 5  # error: [unresolved-attribute]
```

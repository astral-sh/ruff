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
a.x = 1  # error: [possibly-missing-attribute]
a.y = 2  # error: [unresolved-attribute]
```

## Multi-character string

```py
class A:
    __slots__ = "xyz"

a = A()
a.x = 1  # error: [possibly-missing-attribute]
a.y = 2  # error: [possibly-missing-attribute]
a.z = 3  # error: [possibly-missing-attribute]
a.xyz = 4  # error: [unresolved-attribute]
a.q = 5  # error: [unresolved-attribute]
```

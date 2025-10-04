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
print(a.y)  # error: [unresolved-attribute]
```

## Inheritance

```py
class A:
    __slots__ = ("x",)

class B(A):
    __slots__ = ("y",)
    
    def __init__(self, x: int):
        self.x = x

b = B(1)
b.x = 7
b.y = 9  # error: [possibly-missing-attribute]
b.z = 3  # error: [unresolved-attribute]
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

## List slots

```py
class A:
    __slots__ = ["x", "y"]
    
    def __init__(self):
        self.x = 1

a = A()
a.x = 2
a.y = 3  # error: [unresolved-attribute]
a.z = 4  # error: [unresolved-attribute]
```

## Set slots

```py
class A:
    __slots__ = {"x", "y"}
    
    def __init__(self):
        self.x = 1

a = A()
a.x = 2
a.y = 3  # error: [unresolved-attribute]
a.z = 4  # error: [unresolved-attribute]
```

## Slots with `__dict__`

```py
class A:
    __slots__ = ("x", "__dict__")
    
    def __init__(self, foo: int):
        self.foo = foo

a = A(1)
a.foo = 2
a.x = 1
a.bar = 2
```

## Slots with `__weakref__`

```py
import weakref

class A:
    __slots__ = ("x", "__weakref__")

a = A()
ref = weakref.ref(a)
```

## Class variables

```py
class A:
    __slots__ = ("x",)
    y = 10

a = A()
a.x = 1  # error: [possibly-missing-attribute]
print(a.y)
a.y = 20
```

## Property with slots

```py
class A:
    __slots__ = ("x", "y")
    
    def __init__(self, x: str, y: int):
        self.x = x
        self.y = y
    
    @property
    def foo(self) -> str:
        return self.x
    
    @property 
    def bar(self) -> int:
        return self.y

a = A("baz", 77)
print(a.x)
print(a.y)
print(a.foo)
print(a.bar)
a.z = "baz"  # error: [unresolved-attribute]
```

## Multiple inheritance

```py
class A:
    __slots__ = ("x",)

class B:
    __slots__ = ("y",)

class C(A, B):  # error: [instance-layout-conflict]
    __slots__ = ("z",)

c = C()
c.x = 1  # error: [possibly-missing-attribute]
c.y = 2  # error: [possibly-missing-attribute]
c.z = 3  # error: [possibly-missing-attribute]
c.w = 4  # error: [unresolved-attribute]
```

## Inheritance with super

```py
class A:
    __slots__ = ("x",)
    
    def __init__(self, x: str):
        self.x = x

class B(A):
    __slots__ = ("y", "z")
    
    def __init__(self, x: str, y: str, z: str):
        super().__init__(x)
        self.y = y
        self.z = z

b = B("x", "y", "z")
b.x = "a"
b.y = "b"
b.z = "c"
b.q = "q"  # error: [unresolved-attribute]
```

## Slot with default value

```py
class A:
    __slots__ = ("foo",)
    foo: int = 1  # error: [invalid-slots-default] "Attribute `foo` in __slots__ conflicts with class variable"
```

## Invalid non-string in slots

```py
class A:
    __slots__ = (1, 2, 3)  # error: [invalid-slots] "__slots__ items must be strings, not 'int'"
```

## Variable-length builtin subclass

```py
class A(int):
    __slots__ = ("foo",)  # error: [invalid-slots-on-builtin]

class B(bytes):
    __slots__ = ("bar",)  # error: [invalid-slots-on-builtin]

class C(tuple):
    __slots__ = ("baz",)  # error: [invalid-slots-on-builtin]

class D(str):
    __slots__ = ("ham",)  # error: [invalid-slots-on-builtin]
```

## Empty slots on builtin subclass are allowed

```py
class A(int):
    __slots__ = ()

class B(float):
    __slots__ = []
```

## __dict__ unavailable with __slots__

```py
class A:
    pass

a = A()
reveal_type(a.__dict__)  # revealed: dict[str, Any]

class C:
    __slots__ = ("x",)

c = C()
# TODO: this should be `error: [unresolved-attribute]`, but __dict__ is inherited from object in stubs
# reveal_type(c.__dict__)  # error: [unresolved-attribute]
```

## __weakref__ unavailable with __slots__

```py
class A:
    __slots__ = ("x",)

a = A()
a.__weakref__  # error: [unresolved-attribute]
```

## __dict__ in __slots__

```py
class A:
    __slots__ = ("x", "__dict__")

a = A()
reveal_type(a.__dict__)  # revealed: dict[str, Any] | Unknown
```

## __weakref__ in __slots__

```py
class A:
    __slots__ = ("x", "__weakref__")

a = A()
# TODO: this should infer the type from stubs, not Unknown
# error: [possibly-missing-attribute]
reveal_type(a.__weakref__)  # revealed: Unknown
```

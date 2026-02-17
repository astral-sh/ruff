# Augmented assignment

## Basic

```py
x = 3
x -= 1
reveal_type(x)  # revealed: Literal[2]

x = 1.0
x /= 2
reveal_type(x)  # revealed: int | float

x = (1, 2)
x += (3, 4)
reveal_type(x)  # revealed: tuple[Literal[1, 2, 3, 4], ...]
```

## Dunder methods

```py
class C:
    def __isub__(self, other: int) -> str:
        return "Hello, world!"

x = C()
x -= 1
reveal_type(x)  # revealed: str

class C:
    def __iadd__(self, other: str) -> int:
        return 1

x = C()
x += "Hello"
reveal_type(x)  # revealed: int
```

## Unsupported types

<!-- snapshot-diagnostics -->

```py
class C:
    def __isub__(self, other: str) -> int:
        return 42

x = C()
# error: [unsupported-operator] "Operator `-=` is not supported between objects of type `C` and `Literal[1]`"
x -= 1

reveal_type(x)  # revealed: int
```

## Method union

```py
def _(flag: bool):
    class Foo:
        if flag:
            def __iadd__(self, other: int) -> str:
                return "Hello, world!"

        else:
            def __iadd__(self, other: int) -> int:
                return 42

    f = Foo()
    f += 12

    reveal_type(f)  # revealed: str | int
```

## Partially bound `__iadd__`

```py
def _(flag: bool):
    class Foo:
        if flag:
            def __iadd__(self, other: str) -> int:
                return 42

    f = Foo()

    # error: [unsupported-operator] "Operator `+=` is not supported between objects of type `Foo` and `Literal["Hello, world!"]`"
    f += "Hello, world!"

    reveal_type(f)  # revealed: int | Unknown
```

## Partially bound with `__add__`

```py
def _(flag: bool):
    class Foo:
        def __add__(self, other: str) -> str:
            return "Hello, world!"
        if flag:
            def __iadd__(self, other: str) -> int:
                return 42

    f = Foo()
    f += "Hello, world!"

    reveal_type(f)  # revealed: int | str
```

## Partially bound target union

```py
def _(flag1: bool, flag2: bool):
    class Foo:
        def __add__(self, other: int) -> str:
            return "Hello, world!"
        if flag1:
            def __iadd__(self, other: int) -> int:
                return 42

    if flag2:
        f = Foo()
    else:
        f = 42.0
    f += 12

    reveal_type(f)  # revealed: int | str | float
```

## Target union

```py
def _(flag: bool):
    class Foo:
        def __iadd__(self, other: int) -> str:
            return "Hello, world!"

    if flag:
        f = Foo()
    else:
        f = 42
    f += 12

    reveal_type(f)  # revealed: str | Literal[54]
```

## Partially bound target union with `__add__`

```py
def f(flag: bool, flag2: bool):
    class Foo:
        def __add__(self, other: int) -> str:
            return "Hello, world!"
        if flag:
            def __iadd__(self, other: int) -> int:
                return 42

    class Bar:
        def __add__(self, other: int) -> bytes:
            return b"Hello, world!"

        def __iadd__(self, other: int) -> float:
            return 42.0

    if flag2:
        f = Foo()
    else:
        f = Bar()
    f += 12

    reveal_type(f)  # revealed: int | str | float
```

## Attribute target with incompatible result type

```py
class Foo:
    def __add__(self, other: object) -> object:
        return other

class Foo2:
    def __iadd__(self, other: object) -> object:
        return other

class Bar:
    x: Foo
    y: Foo2

b = Bar()
# error: [invalid-assignment] "Object of type `object` is not assignable to attribute `x` of type `Foo`"
b.x += 1
# error: [invalid-assignment] "Object of type `object` is not assignable to attribute `y` of type `Foo2`"
b.y += 1
```

## Attribute target with compatible result type

```py
class Foo:
    def __iadd__(self, other: int) -> "Foo":
        return self

class Bar:
    x: Foo

b = Bar()
b.x += 1
```

## Subscript target with incompatible result type

```py
class Foo:
    def __add__(self, other: object) -> object:
        return other

class Foo2:
    def __iadd__(self, other: object) -> object:
        return other

class Bar:
    def __getitem__(self, key: int) -> Foo:
        return Foo()
    def __setitem__(self, key: int, value: Foo) -> None:
        pass

class Bar2:
    def __getitem__(self, key: int) -> Foo2:
        return Foo2()
    def __setitem__(self, key: int, value: Foo2) -> None:
        pass

b = Bar()
# error: [invalid-assignment] "Invalid subscript assignment with key of type `Literal[0]` and value of type `object` on object of type `Bar`"
b[0] += 1

b2 = Bar2()
# error: [invalid-assignment] "Invalid subscript assignment with key of type `Literal[0]` and value of type `object` on object of type `Bar2`"
b2[0] += 1
```

## Nonexistent attribute target

```py
class Foo: ...

f = Foo()
# error: [unresolved-attribute] "Object of type `Foo` has no attribute `nonexistent`"
f.nonexistent += 1
```

## Subscript target with invalid key type

Both `__getitem__` and `__setitem__` fail for the same reason (wrong key type), so only one
diagnostic should be emitted.

```py
d: dict[str, int] = {}
# error: [invalid-argument-type]
d[1] += 1
```

## Implicit dunder calls on class objects

```py
class Meta(type):
    def __iadd__(cls, other: int) -> str:
        return ""

class C(metaclass=Meta): ...

cls = C
cls += 1

reveal_type(cls)  # revealed: str
```

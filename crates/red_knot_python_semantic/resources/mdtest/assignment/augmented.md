# Augmented assignment

## Basic

```py
x = 3
x -= 1
reveal_type(x)  # revealed: Literal[2]

x = 1.0
x /= 2
reveal_type(x)  # revealed: float
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
    def __iadd__(self, other: str) -> float:
        return 1.0

x = C()
x += "Hello"
reveal_type(x)  # revealed: float
```

## Unsupported types

```py
class C:
    def __isub__(self, other: str) -> int:
        return 42

x = C()
x -= 1

# TODO: should error, once operand type check is implemented
reveal_type(x)  # revealed: int
```

## Method union

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()

class Foo:
    if bool_instance():
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
def bool_instance() -> bool:
    return True

class Foo:
    if bool_instance():
        def __iadd__(self, other: str) -> int:
            return 42

f = Foo()

# TODO: We should emit an `unsupported-operator` error here, possibly with the information
# that `Foo.__iadd__` may be unbound as additional context.
f += "Hello, world!"

reveal_type(f)  # revealed: int
```

## Partially bound with `__add__`

```py
def bool_instance() -> bool:
    return True

class Foo:
    def __add__(self, other: str) -> str:
        return "Hello, world!"
    if bool_instance():
        def __iadd__(self, other: str) -> int:
            return 42

f = Foo()

f += "Hello, world!"

# TODO(charlie): This should be `int | str`, since `__iadd__` may be unbound.
reveal_type(f)  # revealed: int
```

## Partially bound target union

```py
def bool_instance() -> bool:
    return True

class Foo:
    def __add__(self, other: int) -> str:
        return "Hello, world!"
    if bool_instance():
        def __iadd__(self, other: int) -> int:
            return 42

if bool_instance():
    f = Foo()
else:
    f = 42.0
f += 12

# TODO(charlie): This should be `str | int | float`
reveal_type(f)  # revealed: @Todo
```

## Target union

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()

class Foo:
    def __iadd__(self, other: int) -> str:
        return "Hello, world!"

if flag:
    f = Foo()
else:
    f = 42.0
f += 12

# TODO(charlie): This should be `str | float`.
reveal_type(f)  # revealed: @Todo
```

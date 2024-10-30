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
        return "Hello, world!"

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

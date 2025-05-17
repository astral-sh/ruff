# Narrowing by assignment

## Attribute

```py
class A:
    x: int | None = None

a = A()
a.x = 0

reveal_type(a.x)  # revealed: Literal[0]

class C:
    reveal_type(a.x)  # revealed: Literal[0]

[reveal_type(a.x) for _ in range(1)]  # revealed: Literal[0]

def _():
    reveal_type(a.x)  # revealed: Unknown | int | None
```

## Subscript

```py
l: list[int | None] = [None]
l[0] = 0

reveal_type(l[0])  # revealed: Literal[0]

class C:
    reveal_type(l[0])  # revealed: Literal[0]

[reveal_type(l[0]) for _ in range(1)]  # revealed: Literal[0]

def _():
    reveal_type(l[0])  # revealed: int | None
```

## Complex target

```py
class A:
    x: list[int | None] = []

class B:
    a: A | None = None

b = B()
b.a = A()
b.a.x[0] = 0

reveal_type(b.a.x[0])  # revealed: Literal[0]

class C:
    reveal_type(b.a.x[0])  # revealed: Literal[0]

def _():
    # error: [possibly-unbound-attribute]
    reveal_type(b.a.x[0])  # revealed: Unknown | int | None
    # error: [possibly-unbound-attribute]
    reveal_type(b.a.x)  # revealed: Unknown | list[int | None]
    reveal_type(b.a)  # revealed: Unknown | A | None
```

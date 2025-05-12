# Narrowing by assignment

## Attribute

```py
class A:
    x: int | None = None

a = A()
a.x = 0

reveal_type(a.x)  # revealed: Literal[0]
```

## Subscript

```py
l: list[int | None] = [None]
l[0] = 0

reveal_type(l[0])  # revealed: Literal[0]
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
```

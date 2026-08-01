# Intersection type annotations

Intersection types can also be written using the `&` and `~` operators, without importing
`ty_extensions.Intersection`.

## Basic

```toml
[environment]
python-version = "3.14"
```

```py
class A: ...
class B: ...
class C: ...

def _(
    a_and_b: A & B,
    i1: A & B | C,
    i2: A | B & C,
    not_a: ~A,
    nested: A & B & C,
) -> None:
    reveal_type(a_and_b)  # revealed: A & B
    reveal_type(i1)  # revealed: (A & B) | C
    reveal_type(i2)  # revealed: A | (B & C)
    reveal_type(not_a)  # revealed: ~A
    reveal_type(nested)  # revealed: A & B & C
```

The `&` and `~` operators cannot be used in value positions, since that would lead to a runtime
error:

```py
# error: [unsupported-operator] "Operator `&` is not supported between objects of type `<class 'A'>` and `<class 'B'>`"
Invalid1 = A & B
# error: [unsupported-operator] "Unary operator `~` is not supported for object of type `<class 'A'>`"
Invalid2 = ~A
```

## Python 3.13

On Python 3.13 and earlier, annotations are evaluated eagerly by default, so these operators result
in runtime errors. We emit a diagnostic, but still interpret the annotation as an intersection or
negation type:

```toml
[environment]
python-version = "3.13"
```

```py
class A: ...
class B: ...

def _(
    # error: [unsupported-operator] "Operator `&` is not supported between objects of type `<class 'A'>` and `<class 'B'>`"
    a_and_b: A & B,
    # error: [unsupported-operator] "Unary operator `~` is not supported for object of type `<class 'A'>`"
    not_a: ~A,
) -> None:
    reveal_type(a_and_b)  # revealed: A & B
    reveal_type(not_a)  # revealed: ~A
```

## Deferred annotations on Python 3.13

The operators can be used on Python 3.13 if annotations are deferred using a future import:

```toml
[environment]
python-version = "3.13"
```

```py
from __future__ import annotations

class A: ...
class B: ...

def _(a_and_b: A & B, not_a: ~A) -> None:
    reveal_type(a_and_b)  # revealed: A & B
    reveal_type(not_a)  # revealed: ~A
```

Stringified annotations also defer evaluation:

```py
def _(a_and_b: "A & B", not_a: "~A") -> None:
    reveal_type(a_and_b)  # revealed: A & B
    reveal_type(not_a)  # revealed: ~A
```

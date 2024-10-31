# Narrowing in boolean expressions

In `or` expressions, the right-hand side is evaluated only if the left-hand side
is **falsy**. So when the right-hand side is evaluated, we know the left side
has failed.

Similarly, in `and` expressions, the right-hand side is evaluated only if the
left-hand side is **truthy**. So when the right-hand side is evaluated, we know
the left side has succeeded.

## Narrowing in `or`

```py
def bool_instance() -> bool:
    return True

class A: ...

x: A | None = A() if bool_instance() else None

isinstance(x, A) or reveal_type(x)  # revealed: None
x is None or reveal_type(x)  # revealed: A
reveal_type(x)  # revealed: A | None
```

## Narrowing in `and`

```py
def bool_instance() -> bool:
    return True

class A: ...

x: A | None = A() if bool_instance() else None

isinstance(x, A) and reveal_type(x)  # revealed: A
x is None and reveal_type(x)  # revealed: None
reveal_type(x)  # revealed: A | None
```

## Multiple `and` arms

```py
def bool_instance() -> bool:
    return True

class A: ...

x: A | None = A() if bool_instance() else None

bool_instance() and isinstance(x, A) and reveal_type(x)  # revealed: A
isinstance(x, A) and bool_instance() and reveal_type(x)  # revealed: A
reveal_type(x) and isinstance(x, A) and bool_instance()  # revealed: A | None
```

## Multiple `or` arms

```py
def bool_instance() -> bool:
    return True

class A: ...

x: A | None = A() if bool_instance() else None

bool_instance() or isinstance(x, A) or reveal_type(x)  # revealed: None
isinstance(x, A) or bool_instance() or reveal_type(x)  # revealed: None
reveal_type(x) or isinstance(x, A) or bool_instance()  # revealed: A | None
```

## Multiple predicates

```py
def bool_instance() -> bool:
    return True

class A: ...

x: A | None | Literal[1] = A() if bool_instance() else None if bool_instance() else 1

x is None or isinstance(x, A) or reveal_type(x)  # revealed: Literal[1]
```

## Mix of `and` and `or`

```py
def bool_instance() -> bool:
    return True

class A: ...

x: A | None | Literal[1] = A() if bool_instance() else None if bool_instance() else 1

isinstance(x, A) or x is not None and reveal_type(x)  # revealed: Literal[1]
```

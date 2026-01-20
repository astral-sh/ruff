# Public type of undeclared symbols

## Summary

For undeclared public symbols (e.g., class attributes without type annotations), we infer the type
directly from the assigned value(s). This matches the behavior of existing Python type checkers like
Mypy and Pyright:

```py
class Wrapper:
    value = None

wrapper = Wrapper()

reveal_type(wrapper.value)  # revealed: None

# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `value` of type `None`"
wrapper.value = 1
```

Users can always opt in to more flexible behavior by adding type annotations:

```py
class OptionalInt:
    value: int | None = 10

o = OptionalInt()

reveal_type(o.value)  # revealed: int | None

# Incompatible assignments are now caught:
# error: [invalid-assignment] "Object of type `Literal["a"]` is not assignable to attribute `value` of type `int | None`"
o.value = "a"
```

## What is meant by 'public' type?

We apply different semantics depending on whether a symbol is accessed from the same scope in which
it was originally defined, or whether it is accessed from an external scope. Within the same scope
the symbol was defined in, we use the inferred type directly:

```py
class Wrapper:
    value = None

    # Type as seen from the same scope:
    reveal_type(value)  # revealed: None

# Type as seen from another scope:
reveal_type(Wrapper.value)  # revealed: None
```

# Public type of undeclared symbols

## Summary

For undeclared class-level symbols, we infer the type from the assigned value. For example:

```py
class Wrapper:
    value = None

wrapper = Wrapper()

reveal_type(wrapper.value)  # revealed: None

# error: [invalid-assignment]
wrapper.value = 1
```

Since `value` is not declared with a type annotation, we infer its type from the assignment
`value = None`, giving us `None`. Assigning `1` to `wrapper.value` produces an error because `int`
is not assignable to `None`.

Users can opt in to more permissive behavior by adding type annotations:

```py
class Wrapper:
    value: int | None = None

wrapper = Wrapper()

reveal_type(wrapper.value)  # revealed: int | None

# This is now allowed because `int` is assignable to `int | None`
wrapper.value = 1
```

## Example with function call

```py
class Wrapper:
    value = None

def accepts_int(i: int) -> None:
    pass

def f(w: Wrapper) -> None:
    # This is fine: `None` is assignable to `int | None`
    v: int | None = w.value

    # error: [invalid-argument-type] "Argument to function `accepts_int` is incorrect: Expected `int`, found `None`"
    c = accepts_int(w.value)
```

## Example with inferred literal type

```py
class OptionalInt:
    value = 10

o = OptionalInt()

reveal_type(o.value)  # revealed: Literal[10]
```

## Stricter behavior with annotations

Users can always opt in to stricter behavior by adding type annotations:

```py
class OptionalInt:
    value: int | None = 10

o = OptionalInt()

# revealed: int | None
reveal_type(o.value)

# Incompatible assignments are now caught:
# error: "Object of type `Literal["a"]` is not assignable to attribute `value` of type `int | None`"
o.value = "a"
```

## What is meant by 'public' type?

We apply the same semantics whether a symbol is accessed from the same scope in which it was
originally defined, or whether it is accessed from an external scope:

```py
class Wrapper:
    value = None

    # Type as seen from the same scope:
    reveal_type(value)  # revealed: None

# Type as seen from another scope:
reveal_type(Wrapper.value)  # revealed: None
```

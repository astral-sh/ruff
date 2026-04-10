# Public type of undeclared symbols

## Summary

A strict application of the [gradual guarantee] would suggest that all assignments to an unannotated
attribute should be allowed; this could be implemented by unioning all such attributes' inferred
types with `Unknown`. However, in practice this requires too many annotations to achieve sound
typing, and we can heuristically pick the "right" type for unannotated attributes most of the time.

## Promotion

We promote the inferred type of an unannotated attribute to our best guess of its intended public
type. For example, we promote literal types to their nominal supertype, because it is unlikely the
author intended the `value` attribute to always hold the literal `0`:

```py
class Counter:
    def __init__(self) -> None:
        self.value = 0

reveal_type(Counter().value)  # revealed: int
```

## Widening of non-literal singleton types

It's similarly unlikely that an unannotated attribute initialized to a singleton type (like `None`)
is intended to always and only hold the value `None`. But unlike literal types, `None` doesn't have
an obvious candidate super-type to widen to. In this case, we do widen by unioning with `Unknown`:

```py
class Wrapper:
    value = None

wrapper = Wrapper()

reveal_type(wrapper.value)  # revealed: None | Unknown

wrapper.value = 1
```

In this example, the public type is `None | Unknown`, so we also catch uses that are incompatible
with `None`:

```py
def accepts_int(i: int) -> None:
    pass

def f(w: Wrapper) -> None:
    # This is fine
    v: int | None = w.value

    # This function call is incorrect, because `w.value` could be `None`. We therefore emit the following
    # error: "Argument to function `accepts_int` is incorrect: Expected `int`, found `None | Unknown`"
    c = accepts_int(w.value)
```

## Declaring a wider type

Users can always opt in to a wider public type by adding annotations. For the `Wrapper` class, this
could be:

```py
class Wrapper:
    value: int | None = None

w = Wrapper()

# The following public type is now
# revealed: int | None
reveal_type(w.value)

# Incompatible assignments are now caught:
# error: "Object of type `Literal["a"]` is not assignable to attribute `value` of type `int | None`"
w.value = "a"
```

## What is meant by 'public' type?

We apply different semantics depending on whether a symbol is accessed from the same scope in which
it was originally defined, or whether it is accessed from an external scope. External scopes will
see the symbol's "public type", which has been discussed above. But within the same scope the symbol
was defined in, we can often use a narrower literal type before promotion. For example:

```py
class Wrapper:
    value = 10

    # Type as seen from the same scope:
    reveal_type(value)  # revealed: Literal[10]

# Type as seen from another scope:
reveal_type(Wrapper.value)  # revealed: int
```

[gradual guarantee]: https://typing.python.org/en/latest/spec/concepts.html#the-gradual-guarantee

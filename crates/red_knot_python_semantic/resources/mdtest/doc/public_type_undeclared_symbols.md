# Public type of undeclared symbols

## Summary

One major deviation from the behavior of existing Python type checkers is our handling of 'public'
types for undeclared symbols. This is best illustrated with an example:

```py
class Wrapper:
    value = None

wrapper = Wrapper()

reveal_type(wrapper.value)  # revealed: Unknown | None

wrapper.value = 1
```

Mypy and Pyright both infer a type of `None` for the type of `wrapper.value`. Consequently, both
tools emit an error when trying to assign `1` to `wrapper.value`. But there is nothing wrong with
this program. Emitting an error here violates the [gradual guarantee] which states that *"Removing
type annotations (making the program more dynamic) should not result in additional static type
errors."*: If `value` were annotated with `int | None` here, Mypy and Pyright would not emit any
errors.

By inferring `Unknown | None` instead, we allow arbitrary values to be assigned to `wrapper.value`.
This is a deliberate choice to prevent false positive errors on untyped code.

More generally, we infer `Unknown | T_inferred` for undeclared symbols, where `T_inferred` is the
inferred type of the right-hand side of the assignment. This gradual type represents an *unknown*
fully-static type that is *at least as large as* `T_inferred`. It accurately describes our static
knowledge about this type. In the example above, we don't know what values `wrapper.value` could
possibly contain, but we *do know* that `None` is a possibility. This allows us to catch errors
where `wrapper.value` is used in a way that is incompatible with `None`:

```py
def f(w: Wrapper) -> None:
    # This is fine
    v: int | None = w.value

    # This function call is incorrect, because `w.value` could be `None`. We therefore emit the following
    # error: "`Unknown | None` cannot be assigned to parameter 1 (`i`) of function `chr`; expected type `int`"
    c = chr(w.value)
```

## False negatives

In the first example, we demonstrated how our behavior prevents false positives. However, it can
also prevent false negatives. The following example contains a bug, but Mypy and Pyright do not
catch it. To make this a bit more realistic, imagine that `OptionalInt` is imported from an
external, untyped module:

`optional_int.py`:

```py
class OptionalInt:
    value = 10

def reset(o):
    o.value = None
```

It is then used like this:

```py
from typing_extensions import assert_type
from optional_int import OptionalInt, reset

o = OptionalInt()

reset(o)  # Oh no...

# This assertion is incorrect, but Mypy and Pyright do not catch it. We raise the following
# error: "Actual type `Unknown | Literal[10]` is not the same as asserted type `int`"
assert_type(o.value, int)

print(o.value // 2)  # Runtime error!
```

To be fair, we only catch this due to the `assert_type` call. But the type of
`Unknown | Literal[10]` for `o.value` reflects more accurately what the possible values of `o.value`
are.

## Stricter behavior

Users can always opt in to stricter behavior by adding type annotations. For the `OptionalInt`
class, this would probably be:

```py
class OptionalInt:
    value: int | None = 10

# The following public type is now
# revealed: int | None
reveal_type(OptionalInt.value)
```

## What is meant by 'public' type?

We apply different semantics depending on whether a symbol is accessed from the same scope in which
it was originally defined, or whether it is accessed from an external scope. External scopes will
see the symbol's "public type", which has been discussed above. But within the same scope the symbol
was defined in, we use a narrower type of `T_inferred` for undeclared symbols. This is because, from
the perspective of this scope, there is no way that the value of the symbol could have been
reassigned from external scopes. For example:

```py
class Wrapper:
    value = None

    # Type as seen from the same scope:
    reveal_type(value)  # revealed: None

# Type as seen from another scope:
reveal_type(Wrapper.value)  # revealed: Unknown | None
```

[gradual guarantee]: https://typing.readthedocs.io/en/latest/spec/concepts.html#the-gradual-guarantee

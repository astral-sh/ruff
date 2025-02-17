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
def accepts_int(i: int) -> None:
    pass

def f(w: Wrapper) -> None:
    # This is fine
    v: int | None = w.value

    # This function call is incorrect, because `w.value` could be `None`. We therefore emit the following
    # error: "`Unknown | None` cannot be assigned to parameter 1 (`i`) of function `accepts_int`; expected type `int`"
    c = accepts_int(w.value)
```

## Explicit lack of knowledge

The following example demonstrates how Mypy and Pyright's type inference of fully-static types in
these situations can lead to false-negatives, even though everything appears to be (statically)
typed. To make this a bit more realistic, imagine that `OptionalInt` is imported from an external,
untyped module:

`optional_int.py`:

```py
class OptionalInt:
    value = 10

def reset(o):
    o.value = None
```

It is then used like this:

```py
from optional_int import OptionalInt, reset

o = OptionalInt()
reset(o)  # Oh no...

# Mypy and Pyright infer a fully-static type of `int` here, which appears to make the
# subsequent division operation safe -- but it is not. We infer the following type:
reveal_type(o.value)  # revealed: Unknown | Literal[10]

print(o.value // 2)  # Runtime error!
```

We do not catch this mistake either, but we accurately reflect our lack of knowledge about
`o.value`. Together with a possible future type-checker mode that would detect the prevalence of
dynamic types, this could help developers catch such mistakes.

## Stricter behavior

Users can always opt in to stricter behavior by adding type annotations. For the `OptionalInt`
class, this would probably be:

```py
class OptionalInt:
    value: int | None = 10

o = OptionalInt()

# The following public type is now
# revealed: int | None
reveal_type(o.value)

# Incompatible assignments are now caught:
# error: "Object of type `Literal["a"]` is not assignable to attribute `value` of type `int | None`"
o.value = "a"
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

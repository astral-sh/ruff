## What it does

Detects assignments that unsoundly assign a type that is not a [subtype] of the target's declared
type.

This rule is a stricter version of `invalid-assignment`. The rule currently only flags unsound
assignments to variables (excluding attributes and subscripts), but its scope may be expanded in
the future.

This rule has no effect on stub files.

## Why is this bad?

By default, type checkers consider an assignment valid if the inferred type of the assigned value is
[assignable] to the target's declared type. However, this makes it easy for incorrect types to
percolate through your code unexpectedly due to a single expression being inferred as `Any`. This
can easily lead to runtime errors that are not caught by the type checker:

```py
from typing import Any


def returns_any() -> Any:
    return "not an integer"


# error: "Unsound assignment: `Any` is not a subtype of `int`"
my_integer: int = returns_any()

# Fails at runtime, even though the type checker infers both operands as being of type `int`!
my_integer + 42
```

This rule treats ["fully static"][fully-static] declared types as "typed boundaries" for your code.
With this rule enabled, ty would emit an error on the `my_integer: int = returns_any()` assignment,
since the `returns_any()` call is inferred as having type `Any`, and `Any` is not a subtype of `int`.
This helps prevent the unsoundness from spreading far from its original source (in this case, the
return type of the `returns_any` function).

Note that this rule is only applied to assignments where the declared type is
[fully static][fully-static]. It will not trigger if `Any` or `Unknown` appear anywhere in the
declared type, either implicitly or explicitly:

```py
from typing import Any


def returns_any() -> Any:
    return "not an integer"


explicitly_dynamic: Any = returns_any()  # no error
also_dynamic: list[Any] = returns_any()  # no error

# no `unsound-assignment` error, since `list` is implicitly the same as `list[Unknown]`
# (which is what the `missing-type-argument` error is complaining about)
#
# error: [missing-type-argument]
implicitly_dynamic: list = returns_any()
```

This rule works especially well when combined with ty's
`missing-type-argument` rule.

## Examples

```py
from typing import Any


def returns_any() -> Any:
    return 42


# error: "Unsound assignment: `Any` is not a subtype of `int`"
my_integer: int = returns_any()

another_integer: int

# error: "Unsound assignment: `Any` is not a subtype of `int`"
another_integer = returns_any()
```

Narrow the value before assigning it to fix the diagnostics:

```py
from typing import Any


def returns_any() -> Any:
    return 42


value = returns_any()
assert isinstance(value, int)
my_integer: int = value  # no error: `Any & int` is a subtype of `int`
```

## Default level

This rule is disabled by default. It is intended for advanced users wanting additional soundness
checks from their type checker, not for users who have just started to use type checkers on their
Python code.

## See also

- `unsound-return-statement` is a similar rule that triggers on unsound `return` statements rather than unsound assignments
- `unsound-yield` is a similar rule that triggers on unsound `yield` expressions rather than unsound assignments

[assignable]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable
[fully-static]: https://typing.python.org/en/latest/spec/glossary.html#term-fully-static-type
[subtype]: https://typing.python.org/en/latest/spec/glossary.html#term-subtype

## What it does

Detects `return` statements that unsoundly return a type that is not a [subtype] of the function's
annotated return type.

This lint is a stricter version of `invalid-return-type`.

## Why is this bad?

By default, type checkers consider a `return` statement valid if the inferred type of the object
being returned is [assignable] to the annotated return type of the function it's in. However, this
makes it easy for incorrect types to percolate through your code unexpectedly due to a single
expression being inferred as `Any`. This can easily lead to runtime errors that are not caught by
the type checker:

```py
from typing import Any


def returns_any() -> Any:
    return "foo"


def returns_int() -> int:
    # error: "Unsound return statement: `Any` is not a subtype of `int`"
    return returns_any()


# fails at runtime, even though the type checker infers both operands as being of type `int`!
returns_int() + 42
```

This rule allows you to use ["fully static"][fully-static] return types as "typed boundaries" for
your code. With this rule enabled, ty would emit an error on the `return returns_any()` statement
in `returns_int`, since the `returns_any()` call is inferred as having type `Any`, and `Any` is not
a subtype of `int`. This helps prevent the unsoundness from spreading far from its original source
(in this case, the return type of the `returns_any` function).

Note that this rule is only applied to functions annotated as returning
[fully static][fully-static] types. It will not trigger if `Any` or `Unknown` appear anywhere in
your return type, either implicitly or explicitly:

```py
from typing import Any


def returns_any() -> Any:
    return "foo"


# error: [missing-type-argument]
def returns_unparameterized_tuple() -> tuple:
    # no error, since the return type is implicitly `tuple[Unknown, ...]`
    # (which is what the `missing-type-argument` error is complaining about on the line above!)
    return returns_any()


def returns_list_of_any() -> list[Any]:
    # no error, since the return type is explicitly `list[Any]`
    return returns_any()
```

This rule works especially well when combined with ty's
`missing-type-argument` rule, and the Ruff rules [`ANN201`][ann201],
[`ANN202`][ann202], [`ANN204`][ann204], [`ANN205`][ann205], and [`ANN206`][ann206]. Enabling all
these rules at once effectively makes it much less likely that a `return` statement can lead to
unsoundness "leaking" out of a function unless that function has been *explicitly* annotated with
a dynamic type in some way (`-> Any` or `-> tuple[Any]`, for example).

This rule is analogous to mypy's [`no-any-return`][no-any-return] error code, which is enabled by
mypy’s [`--strict`][mypy-strict] mode and can also be enabled on its own using mypy’s
[`--warn-return-any`][warn-return-any] option.

## Examples

```py
from typing import Any


def returns_any() -> Any:
    return 42


def returns_int() -> int:
    # error: "Unsound return statement: `Any` is not a subtype of `int`"
    return returns_any()
```

Narrow the type to a subtype of `int` to fix the diagnostic:

```py
from typing import Any
from typing_extensions import reveal_type


def returns_any() -> Any:
    return 42


def returns_int() -> int:
    my_int = returns_any()
    assert isinstance(my_int, int)
    reveal_type(my_int)  # revealed: Any & int
    return my_int  # no error: `Any & int` is a subtype of `int`
```

## Default level

This rule is disabled by default. It is intended for advanced users wanting additional soundness
checks from their type checker, not for users who have just started to use type checkers on their
Python code.

## See also

- `unsound-yield` is a similar rule that triggers on unsound `yield` expressions rather than unsound `return` statements

[ann201]: https://docs.astral.sh/ruff/rules/missing-return-type-undocumented-public-function/
[ann202]: https://docs.astral.sh/ruff/rules/missing-return-type-private-function/
[ann204]: https://docs.astral.sh/ruff/rules/missing-return-type-special-method/
[ann205]: https://docs.astral.sh/ruff/rules/missing-return-type-static-method/
[ann206]: https://docs.astral.sh/ruff/rules/missing-return-type-class-method/
[assignable]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable
[fully-static]: https://typing.python.org/en/latest/spec/glossary.html#term-fully-static-type
[mypy-strict]: https://mypy.readthedocs.io/en/stable/command_line.html#cmdoption-mypy-strict
[no-any-return]: https://mypy.readthedocs.io/en/stable/error_code_list2.html#code-no-any-return
[subtype]: https://typing.python.org/en/latest/spec/glossary.html#term-subtype
[warn-return-any]: https://mypy.readthedocs.io/en/stable/command_line.html#cmdoption-mypy-warn-return-any

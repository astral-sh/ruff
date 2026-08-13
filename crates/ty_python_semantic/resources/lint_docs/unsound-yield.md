## What it does

Detects `yield` and `yield from` expressions that unsoundly yield a type that is not a [subtype] of
the generator function's annotated yield type.

This lint is a stricter version of `invalid-yield`.

## Why is this bad?

By default, type checkers consider a yielded value valid if its inferred type is [assignable] to the
generator's annotated yield type. However, this
makes it easy for incorrect types to percolate through your code unexpectedly due to a single
expression being inferred as `Any`. This can easily lead to runtime errors that are not caught by
the type checker:

```py
from typing import Any, Generator


def returns_any() -> Any:
    return "not an integer"


def integers() -> Generator[int]:
    # error: "Unsound `yield`: `Any` is not a subtype of `int`"
    yield returns_any()


# Fails at runtime, even though the type checker infers `integers` as yielding only `int`s!
sum(integers())
```

This rule treats [fully static][fully-static] yield types as "typed boundaries" for your code. With this rule enabled, ty would emit an error on the `yield returns_any()` statement
in `integers`, since the `returns_any()` call is inferred as having type `Any`, and `Any` is not
a subtype of `int`. This helps prevent the unsoundness from spreading far from its original source
(in this case, the return type of the `returns_any` function).

Note that this rule is only applied to functions annotated as yielding
[fully static][fully-static] types. It will not trigger if `Any` or `Unknown` appear anywhere in
your function's yield type, either implicitly or explicitly. It will still trigger on functions that have non-fully-static send and/or return types, however:

```py
from typing import Any, Generator


def returns_any() -> Any:
    return "not an integer"


def dynamic_yield_type() -> Generator[Any]:
    yield returns_any()


def static_yield_type() -> Generator[int, Any, Any]:
    # error: "Unsound `yield`: `Any` is not a subtype of `int`"
    yield returns_any()
```

This rule works especially well when combined with ty's
`missing-type-argument` rule, and the Ruff rules [`ANN201`][ann201],
[`ANN202`][ann202], [`ANN204`][ann204], [`ANN205`][ann205], and [`ANN206`][ann206]. Enabling all
these rules at once effectively makes it much less likely that a `yield` expression can lead to
unsoundness "leaking" out of a function unless that function has been *explicitly* annotated with
a dynamic type in some way (`-> Generator[Any]` or `-> Generator[tuple[Any]]`, for example).

## Examples

```py
from typing import Any, Iterator


def returns_any() -> Any:
    return "foo"


def any_iterator() -> Iterator[Any]:
    yield "foo"


def integers() -> Iterator[int]:
    # error: "Unsound `yield`: `Any` is not a subtype of `int`"
    yield returns_any()
    # error: "Unsound `yield from`: `Any` is not a subtype of `int`"
    yield from any_iterator()
```

Narrow the value before yielding it to fix the diagnostics:

```py
from typing import Any, Iterator


def returns_any() -> Any:
    return 42


def any_iterator() -> Iterator[Any]:
    yield "foo"


def integers() -> Iterator[int]:
    value = returns_any()
    assert isinstance(value, int)
    yield value

    for value in any_iterator():
        assert isinstance(value, int)
        yield value
```

## Default level

This rule is disabled by default. It is intended for users who want stricter soundness checks at
generator boundaries.

## See also

- `unsound-return-statement` is a similar rule that triggers on unsound `return` statements rather than unsound `yield` expressions

[ann201]: https://docs.astral.sh/ruff/rules/missing-return-type-undocumented-public-function/
[ann202]: https://docs.astral.sh/ruff/rules/missing-return-type-private-function/
[ann204]: https://docs.astral.sh/ruff/rules/missing-return-type-special-method/
[ann205]: https://docs.astral.sh/ruff/rules/missing-return-type-static-method/
[ann206]: https://docs.astral.sh/ruff/rules/missing-return-type-class-method/
[assignable]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable
[fully-static]: https://typing.python.org/en/latest/spec/glossary.html#term-fully-static-type
[subtype]: https://typing.python.org/en/latest/spec/glossary.html#term-subtype

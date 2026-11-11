## What it does

Checks for `return` statements where the value returned is inconsistent with the function's
return-type annotation.

## Why is this bad?

A caller relying on the annotated return type can receive a value of an unexpected type and fail at
runtime.

## Details and Examples

In a regular (non-generator) function, the value returned by a `return` statement must be assignable
to the function's annotated return type:

```py
def meaning_of_life() -> int:
    return 42  # OK


def bad() -> int:
    return "not a number"  # error: [invalid-explicit-return]
```

This principle applies the same way to async functions:

```py
async def meaning_of_life_async() -> int:
    return 42  # OK


async def bad_async() -> int:
    return "not a number"  # error: [invalid-explicit-return]
```

Note that a bare `return` statement -- without an explicit value being returned -- always returns
`None`, so this can also be flagged by the rule:

```py
def fallible_meaning_of_life(should_answer: bool) -> int | None:
    if not should_answer:
        return  # OK: the return-type annotation allows returning `None`
    return 42


def bad_fallible_function(coinflip: bool) -> int:
    if coinflip:
        # This statement returns `None`, which is not allowed by the return-type annotation:
        return  # error: [invalid-explicit-return]
    return 42
```

For a synchronous generator function annotated with `Generator`, `GeneratorType` or similar,
however, a `return` statement is only valid if it returns a value consistent with the third type
parameter supplied to `Generator`/`GeneratorType`:

```python
from collections.abc import Generator


def gen_func(coinflip: bool) -> Generator[int, None, str]:
    yield 42

    if coinflip:
        # this statement returns `None`, which is not assignable to the third type
        # parameter (`str`) supplied to `Generator` in the return annotation
        return  # error: [invalid-explicit-return]

    yield 42

    # and this returns a `bytes` object, which is also not assignable to `str`
    return b"oops, a bytestring"  # error: [invalid-explicit-return]
```

## Rule history

- v0.0.1-alpha.1: Initially added under the name `invalid-return-type`

- v0.0.14: `empty-body` (since renamed as `invalid-implicit-return-in-function-with-stub-body`) was
    split into a separate rule.

- v0.0.101: `invalid-implicit-return` and `invalid-generator-return-type` were split into separate
    rules, and the rule was renamed as `invalid-explicit-return`. `invalid-return-type` remains as a
    compatibility alias for this rule.

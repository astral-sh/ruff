## What it does

Checks whether the value in a `return` statement is assignable to the function's annotated return
type.

A bare `return` statement is equivalent to `return None` and is checked by this rule. However, cases
where a code path is missing a `return` statement entirely -- leading to execution "falling off" the
end of a function and the function implicitly returning `None` in that code path -- are reported by
either `missing-return-statement` or `empty-body`.

In a generator function annotated with `Generator[YieldType, SendType, ReturnType]`, the value of a
`return` statement must be assignable to `ReturnType`.

## Why is this bad?

Returning an object of a type incompatible with the annotated return type is unsound, and will lead
to ty inferring incorrect types elsewhere.

## Examples

```python
from collections.abc import Generator, AsyncGenerator


def func(coinflip: bool) -> int:
    if coinflip:
        return "a"  # error: [invalid-return-statement]
    else:
        return  # error: [invalid-return-statement]


def gen_func() -> Generator[int, None, str]:
    yield 42
    return b"oops, a bytestring"  # error: [invalid-return-statement]
```

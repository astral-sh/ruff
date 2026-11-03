## What it does

Detects returned values that can't be assigned to the function's annotated return type.

A bare `return` is equivalent to `return None` and is checked by this rule. If execution falls off
the end of a function, Python also returns `None`. ty checks that case with `missing-return` for
nonempty bodies and `empty-body` for empty bodies.

## Why is this bad?

Returning an object of a type incompatible with the annotated return type is unsound, and will lead
to ty inferring incorrect types elsewhere.

## Examples

```python
def func() -> int:
    return "a"  # error: [invalid-return-type]
```

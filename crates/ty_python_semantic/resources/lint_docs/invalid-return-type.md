## What it does

Detects returned values that can't be assigned to the function's annotated return type.

Note that the special case of a function with a non-`None` return type and an empty body
is handled by the separate `empty-body` error code.

## Why is this bad?

Returning an object of a type incompatible with the annotated return type
is unsound, and will lead to ty inferring incorrect types elsewhere.

## Examples

```python
def func() -> int:
    return "a"  # error: [invalid-return-type]
```

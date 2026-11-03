## What it does

Checks whether the value in a `return` statement is assignable to the function's annotated return
type. In a generator function annotated with `Generator[YieldType, SendType, ReturnType]`, the
returned value must be assignable to `ReturnType`.

A bare `return` is equivalent to `return None` and is checked by this rule. If execution falls off
the end of a function, Python also returns `None`. ty checks that case with
`missing-return-statement` for nonempty bodies and `empty-body` for empty bodies.

## Why is this bad?

Returning an object of a type incompatible with the annotated return type is unsound, and will lead
to ty inferring incorrect types elsewhere.

## Examples

```python
def func() -> int:
    return "a"  # error: [invalid-return-statement]
```

## Compatibility

ty 0.0.97 renamed `invalid-return-type` to `invalid-return-statement`. The old name is an alias for
this rule. Missing returns and incompatible generator annotations now have their own rules,
`missing-return-statement` and `invalid-generator-return-type`.

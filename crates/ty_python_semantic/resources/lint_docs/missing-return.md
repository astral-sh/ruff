## What it does

Detects nonempty functions that can reach the end of their body when their return type does not
allow `None`. If a generator annotated with `Generator[YieldType, SendType, ReturnType]` falls off
the end, its final return value is `None`, so `ReturnType` must allow `None` on that path.

## Why is this bad?

When execution reaches the end of a function, Python implicitly returns `None`. A caller relying on
the annotated return type can then fail at runtime.

## Examples

```python
def sign(x: int) -> int:  # error: [missing-return]
    if x < 0:
        return -1
    if x > 0:
        return 1
```

Calling `sign(0)` returns `None`. Add `return 0` at the end of the function to handle that case.

## Compatibility

Before ty 0.0.97, `invalid-return-type` also reported missing returns. If you configured a level for
`invalid-return-type` and want the same level for missing returns, set `missing-return` to that
level too. Replace `# ty: ignore[invalid-return-type]` with `# ty: ignore[missing-return]` on return
annotations where you intended to suppress a missing return.

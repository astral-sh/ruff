## What it does

Checks whether a generator function's return annotation allows the generator object it produces.
This applies to both synchronous and asynchronous generators.

## Why is this bad?

Calling a generator function produces a generator object. A caller that relies on an incompatible
annotation can fail at runtime.

## Examples

```python
def numbers() -> int:  # error: [invalid-generator-return-type]
    yield 1
```

Annotate `numbers` with `Iterator[int]` from `collections.abc` to describe the iterator it returns.

## Compatibility

Before ty 0.0.97, `invalid-return-type` also reported incompatible generator annotations. To keep a
custom severity for these annotations, set `invalid-generator-return-type` to that level. To
suppress the diagnostic on an annotation, use `# ty: ignore[invalid-generator-return-type]`.

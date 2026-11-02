## What it does

Checks for class methods without a return type annotation.

## Why is this bad?

A return annotation documents the result callers can expect and lets ty check that the
implementation returns a compatible value.

## Examples

```python
class Counter:
    step = 1

    @classmethod
    def increment(cls, value: int):  # error: [missing-return-type-class-method]
        return value + cls.step
```

Use instead:

```python
class Counter:
    step = 1

    @classmethod
    def increment(cls, value: int) -> int:
        return value + cls.step
```

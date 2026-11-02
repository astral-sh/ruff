## What it does

Checks for static methods without a return type annotation.

## Why is this bad?

A return annotation documents the result callers can expect and lets ty check that the
implementation returns a compatible value.

## Examples

```python
class Parser:
    @staticmethod
    def parse(text: str):  # error: [missing-return-type-static-method]
        return int(text)
```

Use instead:

```python
class Parser:
    @staticmethod
    def parse(text: str) -> int:
        return int(text)
```

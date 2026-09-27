## What it does

Checks for raw-strings in type annotation positions.

## Why is this bad?

Static analysis tools like ty can't analyze type annotations that use raw-string notation.

## Examples

```python
def test() -> r"int":  # error
    return 1
```

Use instead:

```python
def test() -> "int":
    return 1
```

## What it does
Checks for implicit concatenated strings in type annotation positions.

## Why is this bad?
Static analysis tools like ty can't analyze type annotations that use implicit concatenated strings.

## Examples
```python
def test(): -> "Literal[" "5" "]":
    ...
```

Use instead:
```python
def test(): -> "Literal[5]":
    ...
```

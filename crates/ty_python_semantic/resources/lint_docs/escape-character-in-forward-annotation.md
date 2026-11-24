## What it does

Checks for forward annotations that contain escape characters.

## Why is this bad?

Static analysis tools like ty can't analyze type annotations that contain escape characters.

## Example

```python
def foo() -> "intt\b": ...  # error
```

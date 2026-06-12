## What it does
Checks for calls to non-callable objects.

## Why is this bad?
Calling a non-callable object will raise a `TypeError` at runtime.

## Examples
```python
4()  # TypeError: 'int' object is not callable
```

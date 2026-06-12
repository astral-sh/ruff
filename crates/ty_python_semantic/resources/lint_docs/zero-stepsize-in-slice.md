## What it does
Checks for step size 0 in slices.

## Why is this bad?
A slice with a step size of zero will raise a `ValueError` at runtime.

## Examples
```python
l = list(range(10))
l[1:10:0]  # ValueError: slice step cannot be zero
```

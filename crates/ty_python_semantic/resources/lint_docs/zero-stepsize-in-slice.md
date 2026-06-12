## What it does
Checks for a step size of zero in slices when the operation is known to fail.

## Why is this bad?
Python's built-in sequence types raise a `ValueError` when sliced with a step size of zero.

## Known problems
This check is not exhaustive. It reports zero-step slices for certain built-in sequence
types where the operation is known to fail. A custom `__getitem__` implementation can
accept or reject such a slice, so ty cannot detect every runtime failure.

## Examples
```python
l = list(range(10))
l[1:10:0]  # ValueError: slice step cannot be zero
```

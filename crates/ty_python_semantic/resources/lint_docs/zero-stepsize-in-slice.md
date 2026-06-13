## What it does

Checks for step size 0 in slices.

## Why is this bad?

A slice with a step size of zero will raise a `ValueError` at runtime.

## Examples

```python
values = list(range(10))
# ValueError: slice step cannot be zero
values[1:10:0]

tuple_values = (1, 2, 3)
# ValueError: slice step cannot be zero
tuple_values[1:10:0]  # error
```

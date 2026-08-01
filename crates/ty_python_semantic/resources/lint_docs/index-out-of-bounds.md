## What it does

Checks for attempts to use an out of bounds index to get an item from
a container.

## Why is this bad?

Using an out of bounds index will raise an `IndexError` at runtime.

## Examples

```python
t = (0, 1, 2)
# IndexError: tuple index out of range
t[3]  # error
```

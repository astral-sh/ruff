## What it does

Checks for objects that are not iterable but are used in a context that requires them to be.

## Why is this bad?

Iterating over an object that is not iterable will raise a `TypeError` at runtime.

## Examples

```python
# TypeError: 'int' object is not iterable
for i in 34:  # error
    pass
```

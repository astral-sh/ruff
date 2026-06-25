## What it does

Checks for expressions used in `with` statements
that do not implement the context manager protocol.

## Why is this bad?

Such a statement will raise `TypeError` at runtime.

## Examples

```python
# TypeError: 'int' object does not support the context manager protocol
with 1:  # error
    print(2)
```

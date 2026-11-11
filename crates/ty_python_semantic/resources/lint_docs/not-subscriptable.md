## What it does

Checks for subscript operations that an object may not support, including reading or deleting an
item.

## Why is this bad?

Subscripting an object that does not support it will raise a `TypeError` at runtime.

## Examples

```python
# TypeError: 'int' object is not subscriptable
4[1]  # error
```

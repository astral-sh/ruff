## What it does

Checks for subscripting objects that do not support subscripting.

## Why is this bad?

Subscripting an object that does not support it will raise a `TypeError` at runtime.

## Examples

```python
# TypeError: 'int' object is not subscriptable
4[1]  # error
```

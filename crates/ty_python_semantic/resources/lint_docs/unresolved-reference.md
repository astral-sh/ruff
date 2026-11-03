## What it does

Checks for references to names that are not defined.

When a function uses one of its own local variables before it has a value or after its value has
been deleted, see `unbound-local`.

## Why is this bad?

Evaluating an undefined name raises `NameError` at runtime.

## Example

```python
# NameError: name 'x' is not defined
print(x)  # error
```

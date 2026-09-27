## What it does

Checks for references to names that are not defined.

## Why is this bad?

Using an undefined variable will raise a `NameError` at runtime.

## Example

```python
# NameError: name 'x' is not defined
print(x)  # error
```

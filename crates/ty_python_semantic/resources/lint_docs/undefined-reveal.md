## What it does

Checks for calls to `reveal_type` without importing it.

## Why is this bad?

Using `reveal_type` without importing it will raise a `NameError` at runtime.

## Examples

```python
# NameError: name 'reveal_type' is not defined
# error
reveal_type(1)  # revealed: Literal[1]
```

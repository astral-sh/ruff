## What it does

Checks for calls to `reveal_type` without importing it.

## Why is this bad?

Using `reveal_type` without importing it will raise a `NameError` at runtime.

## Examples

```python
reveal_type(1)  # NameError: name 'reveal_type' is not defined
```

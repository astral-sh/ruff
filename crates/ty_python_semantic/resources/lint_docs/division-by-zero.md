## What it does

It detects division by zero.

## Why is this bad?

Dividing by zero raises a `ZeroDivisionError` at runtime.

## Rule status

This rule is currently disabled by default because of the number of
false positives it can produce.

## Examples

```python
5 / 0  # error
```

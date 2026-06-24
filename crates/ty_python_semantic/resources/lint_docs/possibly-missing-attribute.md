## What it does

Checks for possibly missing attributes.

## Why is this bad?

Attempting to access a missing attribute will raise an `AttributeError` at runtime.

## Rule status

This rule is currently disabled by default because of the number of
false positives it can produce.

## Examples

```python
class A:
    if __name__ == "__main__":
        c = 0


# AttributeError: type object 'A' has no attribute 'c'
A.c  # error
```

## What it does

Checks for unresolved attributes.

## Why is this bad?

Accessing an unbound attribute will raise an `AttributeError` at runtime.
An unresolved attribute is not guaranteed to exist from the type alone,
so this could also indicate that the object is not of the type that the user expects.

## Examples

```python
class A: ...


# AttributeError: 'A' object has no attribute 'foo'
A().foo  # error
```

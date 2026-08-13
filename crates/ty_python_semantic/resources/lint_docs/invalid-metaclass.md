## What it does

Checks for arguments to `metaclass=` that are invalid.

## Why is this bad?

Python allows arbitrary expressions to be used as the argument to `metaclass=`.
These expressions, however, need to be callable and accept the same arguments
as `type.__new__`.

## Example

```python
# TypeError: 'int' object is not callable
class B(metaclass=42): ...  # error
```

## References

- [Python documentation: Metaclasses](https://docs.python.org/3/reference/datamodel.html#metaclasses)

## What it does

Checks for class definitions that will fail due to non-callable `__init_subclass__`
methods.

## Why is this bad?

If a class defines a non-callable `__init_subclass__` method/attribute, any attempt
to subclass that class will raise a `TypeError` at runtime.

## Examples

```python
class Super:
    __init_subclass__ = None


class Sub(Super): ...  # error: [non-callable-init-subclass]
```

## References

- [Python data model: Customizing class creation](https://docs.python.org/3/reference/datamodel.html#customizing-class-creation)

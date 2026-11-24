## What it does

Detects `super()` calls where:

- the first argument is not a valid class literal, or
- the second argument is not an instance or subclass of the first argument.

## Why is this bad?

`super(type, obj)` expects:

- the first argument to be a class,
- and the second argument to satisfy one of the following:
    - `isinstance(obj, type)` is `True`
    - `issubclass(obj, type)` is `True`

Violating this relationship will raise a `TypeError` at runtime.

## Examples

```python
class A: ...


class B(A): ...


super(A, B())  # it's okay! `A` satisfies `isinstance(B(), A)`

# `A()` is not a class
super(A(), B())  # error

# `A()` does not satisfy `isinstance(A(), B)`
super(B, A())  # error
# `A` does not satisfy `issubclass(A, B)`
super(B, A)  # error
```

## References

- [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)

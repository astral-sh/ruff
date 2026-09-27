## What it does

Checks for classes with an inconsistent [method resolution order] (MRO).

## Why is this bad?

Classes with an inconsistent MRO will raise a `TypeError` at runtime.

## Examples

```python
class A: ...


class B(A): ...


# TypeError: Cannot create a consistent method resolution order
class C(A, B): ...  # error
```

[method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order

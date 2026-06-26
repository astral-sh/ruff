## What it does

Checks for class definitions in stub files that inherit
(directly or indirectly) from themselves.

## Why is it bad?

Although forward references are natively supported in stub files,
inheritance cycles are still disallowed, as it is impossible to
resolve a consistent [method resolution order] for a class that
inherits from itself.

## Examples

`foo.pyi`:

```pyi
class A(B): ...  # error
class B(A): ...  # error
```

[method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order

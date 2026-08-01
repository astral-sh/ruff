## What it does

Checks for class definitions with duplicate bases.

## Why is this bad?

Class definitions with duplicate bases raise `TypeError` at runtime.

## Examples

```python
class A: ...


# TypeError: duplicate base class
class B(A, A): ...  # error
```

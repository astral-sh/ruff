# missing-return-type-private-function (ANN202)

Derived from the **flake8-annotations** linter.

## What it does
Checks that private functions and methods have return type annotations.

## Why is this bad?
Type annotations are a good way to document the return types of functions. They also
help catch bugs, when used alongside a type checker, by ensuring that the types of
any returned values, and the types expected by callers, match expectation.

## Example
```python
def _add(a, b):
    return a + b
```

Use instead:
```python
def _add(a: int, b: int) -> int:
    return a + b
```
# missing-type-function-argument (ANN001)

Derived from the **flake8-annotations** linter.

## What it does
Checks that function arguments have type annotations.

## Why is this bad?
Type annotations are a good way to document the types of function arguments. They also
help catch bugs, when used alongside a type checker, by ensuring that the types of
any provided arguments match expectation.

## Example
```python
def foo(x):
    ...
```

Use instead:
```python
def foo(x: int):
    ...
```
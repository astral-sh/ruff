# missing-type-kwargs (ANN003)

Derived from the **flake8-annotations** linter.

## What it does
Checks that function `**kwargs` arguments have type annotations.

## Why is this bad?
Type annotations are a good way to document the types of function arguments. They also
help catch bugs, when used alongside a type checker, by ensuring that the types of
any provided arguments match expectation.

## Example
```python
def foo(**kwargs):
    ...
```

Use instead:
```python
def foo(**kwargs: int):
    ...
```
## What it does

Checks for function parameters without a type annotation, including `*args` and `**kwargs`.

For overloaded functions, ty checks the overload signatures and skips the implementation.

## Why is this bad?

Without an annotation, ty cannot use the parameter's intended type to check calls to the function.

## Examples

```python
def greet(name):  # error: [missing-parameter-annotation]
    return f"Hello, {name}!"
```

Use instead:

```python
def greet(name: str):
    return f"Hello, {name}!"
```

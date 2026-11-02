## What it does

Checks for positional and keyword-only parameters without a type annotation.

For overloaded functions, ty checks the overload signatures and skips the implementation.

## Why is this bad?

Without an annotation, ty cannot use the parameter's intended type to check calls to the function.

## Examples

```python
def greet(name):  # error: [missing-type-function-argument]
    return f"Hello, {name}!"
```

Use instead:

```python
def greet(name: str):
    return f"Hello, {name}!"
```

## See also

- `missing-type-args` checks variadic positional parameters (`*args`).
- `missing-type-kwargs` checks variadic keyword parameters (`**kwargs`).

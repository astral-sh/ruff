## What it does

Checks for variadic positional parameters (`*args`) without a type annotation.

For overloaded functions, ty checks the overload signatures and skips the implementation.

## Why is this bad?

Annotating `*args` lets ty check the type of each positional argument collected by the parameter.

## Examples

```python
def total(*values):  # error: [missing-type-args]
    return sum(values)
```

Use instead:

```python
def total(*values: int):
    return sum(values)
```

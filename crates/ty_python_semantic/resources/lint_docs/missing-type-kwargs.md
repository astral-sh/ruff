## What it does

Checks for variadic keyword parameters (`**kwargs`) without a type annotation.

For overloaded functions, ty checks the overload signatures and skips the implementation.

## Why is this bad?

Annotating `**kwargs` lets ty check the type of each keyword argument collected by the parameter.

## Examples

```python
def headers(**fields):  # error: [missing-type-kwargs]
    return fields
```

Use instead:

```python
def headers(**fields: str):
    return fields
```

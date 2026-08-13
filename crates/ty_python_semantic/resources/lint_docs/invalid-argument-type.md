## What it does

Detects call arguments whose type is not assignable to the corresponding typed parameter.

## Why is this bad?

Passing an argument of a type the function (or callable object) does not accept violates
the expectations of the function author and may cause unexpected runtime errors within the
body of the function.

## Examples

```python
def func(x: int): ...


func("foo")  # error: [invalid-argument-type]
```

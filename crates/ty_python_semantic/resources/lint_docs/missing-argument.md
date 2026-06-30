## What it does

Checks for missing required arguments in a call.

## Why is this bad?

Failing to provide a required argument will raise a `TypeError` at runtime.

## Examples

```python
def func(x: int): ...


# TypeError: func() missing 1 required positional argument: 'x'
func()  # error
```

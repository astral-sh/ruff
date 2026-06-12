## What it does

Checks whether a variable has been declared as two conflicting types.

## Why is this bad

A variable with two conflicting declarations likely indicates a mistake.
Moreover, it could lead to incorrect or ill-defined type inference for
other code that relies on these variables.

## Examples

```python
if __name__ == "__main__":
    a: int
else:
    a: str

a = 1  # error
```

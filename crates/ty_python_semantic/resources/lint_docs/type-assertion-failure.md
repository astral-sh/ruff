## What it does
Checks for `assert_type()` and `assert_never()` calls where the actual type
is not the same as the asserted type.

## Why is this bad?
`assert_type()` allows confirming the inferred type of a certain value.

## Example

```python
def _(x: int):
    assert_type(x, int)  # fine
    assert_type(x, str)  # error: Actual type does not match asserted type
```

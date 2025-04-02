# Narrowing with assert statements

## `assert` on singletons

```py
def _(x: str | None):
    assert x is not None
    reveal_type(x)  # revealed: str
```

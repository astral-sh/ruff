# String annotations

```py
def f() -> "int":
    return 1

# TODO: We do not support string annotations, but we should not panic if we encounter them
reveal_type(f())  # revealed: @Todo
```

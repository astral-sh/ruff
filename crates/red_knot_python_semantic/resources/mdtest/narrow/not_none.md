# `is not None` narrowing

```py
x = None if flag else 1
if x is not None:
    reveal_type(x)  # revealed: Literal[1]

reveal_type(x)  # revealed: None | Literal[1]
```

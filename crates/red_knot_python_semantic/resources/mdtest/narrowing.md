# Type Narrowing

## `… is None`

```py
x = None if flag else 1

reveal_type(x)  # revealed: None | Literal[1]

if x is None:
    # TODO: this should be None
    reveal_type(x)  # revealed: None | Literal[1] & None
else:
    # TODO: this should be Literal[1]
    reveal_type(x)  # revealed: None | Literal[1]
```

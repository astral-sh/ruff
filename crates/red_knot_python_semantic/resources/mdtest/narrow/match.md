# Narrowing for `match` statements

## Single `match` pattern

```py
x = None if flag else 1
reveal_type(x)  # revealed: None | Literal[1]

y = 0

match x:
    case None:
        y = x

reveal_type(y)  # revealed: Literal[0] | None
```

# Conditionals

## Narrowing

### `is != None`

```py
x = None if flag else 1
y = 0
if x != None:
    y = x

reveal_type(x)  # revealed: None | Literal[1]
reveal_type(y)  # revealed: Literal[0, 1]
```

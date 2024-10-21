# Narrowing for `!=` conditionals

## `x != None`

```py
x = None if flag else 1

if x != None:
    reveal_type(x)  # revealed: Literal[1]
```

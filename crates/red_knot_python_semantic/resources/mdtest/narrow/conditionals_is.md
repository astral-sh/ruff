# Narrowing for `is` conditionals

## `is None`

```py
x = None if flag else 1

if x is None:
    # TODO the following should be simplified to 'None'
    reveal_type(x)  # revealed: None | Literal[1] & None

reveal_type(x)  # revealed: None | Literal[1]
```

## `is` for other types

```py
class A:
    ...

x = A()
y = x if flag else None

if y is x:
    # TODO the following should be simplified to 'A'
    reveal_type(y)  # revealed: A | None & A

reveal_type(y)  # revealed: A | None
```

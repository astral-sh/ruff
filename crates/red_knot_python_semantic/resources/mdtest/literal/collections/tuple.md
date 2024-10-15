# Tuples

## Empty tuple

```py
x = ()
reveal_type(x)  # revealed: tuple[()]
```

## Heterogeneous tuple

```py
x = (1, 'a')
y = (1, (2, 3))
z = (x, 2)

reveal_type(x)  # revealed: tuple[Literal[1], Literal["a"]]
reveal_type(y)  # revealed: tuple[Literal[1], tuple[Literal[2], Literal[3]]]
reveal_type(z)  # revealed: tuple[tuple[Literal[1], Literal["a"]], Literal[2]]
```

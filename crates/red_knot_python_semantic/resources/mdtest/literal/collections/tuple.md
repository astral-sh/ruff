# Tuples

## Empty tuple

```py
reveal_type(())  # revealed: tuple[()]
```

## Heterogeneous tuple

```py
x = (1, 'a')
reveal_type(x)  # revealed: tuple[Literal[1], Literal["a"]]

y = (1, (2, 3))
reveal_type(y)  # revealed: tuple[Literal[1], tuple[Literal[2], Literal[3]]]

z = (x, 2)
reveal_type(z)  # revealed: tuple[tuple[Literal[1], Literal["a"]], Literal[2]]
```

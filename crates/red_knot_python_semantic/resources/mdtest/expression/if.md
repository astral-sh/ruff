# If expression

## Union

```py
def bool_instance() -> bool:
    return True

reveal_type(1 if bool_instance() else 2)  # revealed: Literal[1, 2]
```

## Statically known branches

```py
reveal_type(1 if True else 2)  # revealed: Literal[1]
reveal_type(1 if "not empty" else 2)  # revealed: Literal[1]
reveal_type(1 if (1,) else 2)  # revealed: Literal[1]
reveal_type(1 if 1 else 2)  # revealed: Literal[1]

reveal_type(1 if False else 2)  # revealed: Literal[2]
reveal_type(1 if None else 2)  # revealed: Literal[2]
reveal_type(1 if "" else 2)  # revealed: Literal[2]
reveal_type(1 if 0 else 2)  # revealed: Literal[2]
```

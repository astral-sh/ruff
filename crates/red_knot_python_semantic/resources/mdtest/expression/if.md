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

## Leaked Narrowing Constraint

(issue #14588)

The test inside an if expression should not affect the scope outside of it.

```py
def bool_instance() -> bool:
    return True

x: Literal[42, "hello"] = 42 if bool_instance() else "hello"

reveal_type(x)  # revealed: Literal[42] | Literal["hello"]

_ = ... if isinstance(x, str) else ...

reveal_type(x)  # revealed: Literal[42] | Literal["hello"]
```

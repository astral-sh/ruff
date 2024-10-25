# Comparison: Integers

## Integer literals

```py
reveal_type(1 == 1 == True)  # revealed: Literal[True]
reveal_type(1 == 1 == 2 == 4)  # revealed: Literal[False]
reveal_type(False < True <= 2 < 3 != 6)  # revealed: Literal[True]
reveal_type(1 < 1)  # revealed: Literal[False]
reveal_type(1 > 1)  # revealed: Literal[False]
reveal_type(1 is 1)  # revealed: bool
reveal_type(1 is not 1)  # revealed: bool
reveal_type(1 is 2)  # revealed: Literal[False]
reveal_type(1 is not 7)  # revealed: Literal[True]
# TODO: should be Unknown, once type-checking for rich comparison operands is implemented
reveal_type(1 <= "" and 0 < 1)  # revealed: bool
```

## Integer instance

```py
def int_instance() -> int:
    return 42

reveal_type(1 == int_instance())  # revealed: bool
reveal_type(9 < int_instance())  # revealed: bool
reveal_type(int_instance() < int_instance())  # revealed: bool
```

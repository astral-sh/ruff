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
# TODO: should be Unknown, and emit diagnostic, once we check call argument types
reveal_type(1 <= "" and 0 < 1)  # revealed: bool
```

## Integer instance

```py
# TODO: implement lookup of `__eq__` on typeshed `int` stub.
def _(a: int, b: int):
    reveal_type(1 == a)  # revealed: bool
    reveal_type(9 < a)  # revealed: bool
    reveal_type(a < b)  # revealed: bool
```

# Narrowing for nested conditionals

## Multiple negative contributions

```py
def int_instance() -> int: ...

x = int_instance()

if x != 1:
    if x != 2:
        if x != 3:
            reveal_type(x)  # revealed: int & ~Literal[1] & ~Literal[2] & ~Literal[3]
```

## Multiple negative contributions with simplification

```py
def bool_instance() -> bool:
    return True

flag1, flag2 = bool_instance(), bool_instance()
x = 1 if flag1 else 2 if flag2 else 3

if x != 1:
    reveal_type(x)  # revealed: Literal[2, 3]
    if x != 2:
        reveal_type(x)  # revealed: Literal[3]
```

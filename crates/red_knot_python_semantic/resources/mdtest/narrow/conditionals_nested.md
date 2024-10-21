# Narrowing for nested conditionals

```py
def int_instance() -> int: ...


x = int_instance()

if x != 1:
    if x != 2:
        if x != 3:
            reveal_type(x)  # revealed: int & ~Literal[1] & ~Literal[2] & ~Literal[3]
```

# Narrowing for conditionals with elif and else

## Positive contributions become negative in elif-else blocks

```py
def int_instance() -> int: ...

x = int_instance()

if x == 1:
    reveal_type(x)  # revealed: int
elif x == 2:
    reveal_type(x)  # revealed: int & ~Literal[1]
elif x != 3:
    reveal_type(x)  # revealed: int & ~Literal[1] & ~Literal[2] & ~Literal[3]
```

## Positive contributions become negative in elif-else blocks, with simplification

```py
x = 1 if flag1 else 2 if flag2 else 3

if x == 1:
    reveal_type(x)  # revealed: Literal[1, 2, 3]
elif x == 2:
    reveal_type(x)  # revealed: Literal[2, 3]
else:
    reveal_type(x)  # revealed: Literal[3]
```

## Multiple negative contributions using elif, with simplification

```py
x = 1 if flag1 else 2 if flag2 else 3

if x != 1:
    reveal_type(x)  # revealed: Literal[2, 3]
elif x != 2:
    # 1 is still a possibility here, as we don't narrow basing `==` check
    reveal_type(x)  # revealed: Literal[1, 3]
elif x == 3:
    # 2 and 3 are still valid here, as we don't narrow basing `==` check
    reveal_type(x)  # revealed: Literal[1, 2, 3]
else:
    reveal_type(x)  # revealed: Literal[1, 2]
```

# Narrowing for conditionals with elif and else

## Positive contributions become negative in elif-else blocks

```py
def _(x: int):
    if x == 1:
        # cannot narrow; could be a subclass of `int`
        reveal_type(x)  # revealed: int
    elif x == 2:
        reveal_type(x)  # revealed: int & ~Literal[1]
    elif x != 3:
        reveal_type(x)  # revealed: int & ~Literal[1] & ~Literal[2] & ~Literal[3]
```

## Positive contributions become negative in elif-else blocks, with simplification

```py
def _(flag1: bool, flag2: bool):
    x = 1 if flag1 else 2 if flag2 else 3

    if x == 1:
        reveal_type(x)  # revealed: Literal[1]
    elif x == 2:
        reveal_type(x)  # revealed: Literal[2]
    else:
        reveal_type(x)  # revealed: Literal[3]
```

## Multiple negative contributions using elif, with simplification

```py
def _(flag1: bool, flag2: bool):
    x = 1 if flag1 else 2 if flag2 else 3

    if x != 1:
        reveal_type(x)  # revealed: Literal[2, 3]
    elif x != 2:
        reveal_type(x)  # revealed: Literal[1]
    elif x == 3:
        reveal_type(x)  # revealed: Never
    else:
        reveal_type(x)  # revealed: Never
```

## Assignment expressions

```py
def f() -> int | str | None: ...

if isinstance(x := f(), int):
    reveal_type(x)  # revealed: int
elif isinstance(x, str):
    reveal_type(x)  # revealed: str & ~int
else:
    reveal_type(x)  # revealed: None
```

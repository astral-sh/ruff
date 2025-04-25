# Narrowing for nested conditionals

## Multiple negative contributions

```py
def _(x: int):
    if x != 1:
        if x != 2:
            if x != 3:
                reveal_type(x)  # revealed: int & ~Literal[1] & ~Literal[2] & ~Literal[3]
```

## Multiple negative contributions with simplification

```py
def _(flag1: bool, flag2: bool):
    x = 1 if flag1 else 2 if flag2 else 3

    if x != 1:
        reveal_type(x)  # revealed: Literal[2, 3]
        if x != 2:
            reveal_type(x)  # revealed: Literal[3]
```

## elif-else blocks

```py
def _(flag1: bool, flag2: bool):
    x = 1 if flag1 else 2 if flag2 else 3

    if x != 1:
        reveal_type(x)  # revealed: Literal[2, 3]
        if x == 2:
            reveal_type(x)  # revealed: Literal[2]
        elif x == 3:
            reveal_type(x)  # revealed: Literal[3]
        else:
            reveal_type(x)  # revealed: Never

    elif x != 2:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: Never
```

## Cross-scope narrowing

```py
from typing import Literal

def f(x: str | None):
    def _():
        if x is not None:
            reveal_type(x)  # revealed: str

    class C:
        if x is not None:
            reveal_type(x)  # revealed: str

    # TODO: should be str
    [reveal_type(x) for _ in range(1) if x is not None]  # revealed: str | None

    if x is not None:
        def _():
            # If there is a possibility that `x` may be rewritten after this function definition,
            # the constraint `x is not None` outside the function is no longer be applicable for narrowing.
            reveal_type(x)  # revealed: str | None

        class C:
            reveal_type(x)  # revealed: str

        [reveal_type(x) for _ in range(1)]  # revealed: str

def g(x: str | Literal[1] | None):
    class C:
        if x is not None:
            def _():
                if x != 1:
                    reveal_type(x)  # revealed: str | None

            class D:
                if x != 1:
                    reveal_type(x)  # revealed: str

            # TODO: should be str
            [reveal_type(x) for _ in range(1) if x != 1]  # revealed: str | Literal[1]
```

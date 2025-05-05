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

Narrowing constraints are also valid in eager nested scopes (however, because class variables are
not visible from nested scopes, constraints on those variables are invalid).

Currently they are assumed to be invalid in lazy nested scopes since there is a possibility that the
constraints may no longer be valid due to a "time lag". However, it may be possible to determine
that some of them are valid by performing a more detailed analysis (e.g. checking that the narrowing
target has not changed in all places where the function is called).

### Narrowing constraints introduced in eager nested scopes

```py
g: str | None = "a"

def f(x: str | None):
    def _():
        if x is not None:
            reveal_type(x)  # revealed: str

        if not isinstance(x, str):
            reveal_type(x)  # revealed: None

        if g is not None:
            reveal_type(g)  # revealed: str

    class C:
        if x is not None:
            reveal_type(x)  # revealed: str

        if not isinstance(x, str):
            reveal_type(x)  # revealed: None

        if g is not None:
            reveal_type(g)  # revealed: str

    # TODO: should be str
    # This could be fixed if we supported narrowing with if clauses in comprehensions.
    [reveal_type(x) for _ in range(1) if x is not None]  # revealed: str | None
```

### Narrowing constraints introduced in the outer scope

```py
g: str | None = "a"

def f(x: str | None):
    if x is not None:
        def _():
            # If there is a possibility that `x` may be rewritten after this function definition,
            # the constraint `x is not None` outside the function is no longer be applicable for narrowing.
            reveal_type(x)  # revealed: str | None

        class C:
            reveal_type(x)  # revealed: str

        [reveal_type(x) for _ in range(1)]  # revealed: str

    if g is not None:
        def _():
            reveal_type(g)  # revealed: str | None

        class D:
            reveal_type(g)  # revealed: str

        [reveal_type(g) for _ in range(1)]  # revealed: str
```

### Narrowing constraints introduced in multiple scopes

```py
from typing import Literal

g: str | Literal[1] | None = "a"

def f(x: str | Literal[1] | None):
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

        if g is not None:
            def _():
                if g != 1:
                    reveal_type(g)  # revealed: str | None

            class D:
                if g != 1:
                    reveal_type(g)  # revealed: str
```

### Narrowing constraints with bindings in class scope, and nested scopes

```py
from typing import Literal

g: str | Literal[1] | None = "a"

def f(flag: bool):
    class C:
        (g := None) if flag else (g := None)
        # `g` is always bound here, so narrowing checks don't apply to nested scopes
        if g is not None:
            class F:
                reveal_type(g)  # revealed: str | Literal[1] | None

    class C:
        # this conditional binding leaves "unbound" visible, so following narrowing checks apply
        None if flag else (g := None)

        if g is not None:
            class F:
                reveal_type(g)  # revealed: str | Literal[1]

            # This class variable is not visible from the nested class scope.
            g = None

            # This additional constraint is not relevant to nested scopes, since it only applies to
            # a binding of `g` that they cannot see:
            if g is None:
                class E:
                    reveal_type(g)  # revealed: str | Literal[1]
```

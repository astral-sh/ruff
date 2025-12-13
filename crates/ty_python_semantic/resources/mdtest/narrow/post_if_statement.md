# Consolidating narrowed types after if statement

## After if-else statements, narrowing has no effect if the variable is not mutated in any branch

```py
def _(x: int | None):
    if x is None:
        pass
    else:
        pass

    reveal_type(x)  # revealed: int | None
```

## Narrowing can have a persistent effect if the variable is mutated in one branch

```py
def _(x: int | None):
    if x is None:
        x = 10
    else:
        pass

    reveal_type(x)  # revealed: int
```

## An if statement without an explicit `else` branch is equivalent to one with a no-op `else` branch

```py
def _(x: int | None, y: int | None):
    if x is None:
        x = 0

    if y is None:
        pass

    reveal_type(x)  # revealed: int
    reveal_type(y)  # revealed: int | None
```

## An if-elif without an explicit else branch is equivalent to one with an empty else branch

```py
def _(x: int | None):
    if x is None:
        x = 0
    elif x > 50:
        x = 50

    reveal_type(x)  # revealed: int
```

## Narrowing after a NoReturn call in one branch

When a branch calls a function that returns `NoReturn`/`Never`, we know that branch terminates and
doesn't contribute to the type after the if statement.

```py
import sys

def _(val: int | None):
    if val is None:
        sys.exit()
    # After the if statement, val cannot be None because that case
    # would have called sys.exit() and never reached here
    reveal_type(val)  # revealed: int
```

This also works when the NoReturn function is called in the else branch:

```py
import sys

def _(val: int | None):
    if val is not None:
        pass
    else:
        sys.exit()
    reveal_type(val)  # revealed: int
```

And for elif branches:

```py
import sys

def _(val: int | str | None):
    if val is None:
        sys.exit()
    elif isinstance(val, int):
        pass
    else:
        sys.exit()
    # TODO: Should be `int`, but we don't yet fully support narrowing after NoReturn in elif chains
    reveal_type(val)  # revealed: int | str
```

## Narrowing from assert should not affect reassigned variables

When a variable is reassigned after an `assert`, the narrowing from the assert should not apply to
the new value. The assert condition was about the old value, not the new one.

```py
def foo(arg: int) -> int | None:
    return None

def bar() -> None:
    v = foo(1)
    assert v is None

    v = foo(2)
    # v was reassigned, so the assert narrowing shouldn't apply
    reveal_type(v)  # revealed: int | None
```

## Narrowing from NoReturn should not affect reassigned variables

Similar to assert, when a variable is narrowed due to a NoReturn call in one branch and then
reassigned, the narrowing should only apply before the reassignment, not after.

```py
import sys

def foo() -> int | None:
    return 3

def bar():
    v = foo()
    if v is None:
        sys.exit()
    reveal_type(v)  # revealed: int

    v = foo()
    # v was reassigned, so the NoReturn narrowing shouldn't apply
    reveal_type(v)  # revealed: int | None
```

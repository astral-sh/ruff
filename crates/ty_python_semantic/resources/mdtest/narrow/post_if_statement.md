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

## Narrowing is preserved when a terminal branch prevents a path from flowing through

When one branch of an if/elif/else is terminal (e.g. contains `return`), narrowing from the
non-terminal branches is preserved after the merge point.

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A):
        pass
    elif isinstance(x, B):
        pass
    else:
        return

    # Only the if-branch (A) and elif-branch (B) flow through.
    # The else-branch returned, so its narrowing doesn't participate.
    reveal_type(x)  # revealed: B | (A & ~B)
```

## Narrowing is preserved with multiple terminal branches

```py
class A: ...
class B: ...
class C: ...
class D: ...

def _(x: A | B | C | D):
    if isinstance(x, A):
        return
    elif isinstance(x, B):
        pass
    elif isinstance(x, C):
        pass
    else:
        return

    # Only the elif-B and elif-C branches flow through.
    reveal_type(x)  # revealed: (C & ~A) | (B & ~A & ~C)
```

## Multiple sequential if-statements don't leak narrowing

After a complete if/else where both branches flow through (no terminal), narrowing should be
cancelled out at the merge point.

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A):
        pass
    else:
        pass

    # Narrowing cancels out: both paths flow, so type is unchanged.
    reveal_type(x)  # revealed: A | B | C

    if isinstance(x, B):
        pass
    else:
        pass

    # Second if-statement's narrowing also cancels out.
    reveal_type(x)  # revealed: A | B | C
```

## Narrowing after a `NoReturn` call in one branch

When a branch calls a function that returns `NoReturn`/`Never`, we know that branch terminates and
doesn't contribute to the type after the if statement.

```py
import sys

def _(val: int | None):
    if val is None:
        sys.exit()
    reveal_type(val)  # revealed: int
```

This also works when the `NoReturn` function is called in the else branch:

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
    reveal_type(val)  # revealed: int
```

## Narrowing through always-true branches

When a terminal (`return`) is inside an always-true branch, narrowing propagates through because the
else-branch is unreachable and contributes `Never` to the union.

```py
def _(x: int | None):
    if True:
        if x is None:
            return
        reveal_type(x)  # revealed: int
    reveal_type(x)  # revealed: int
```

```py
def _(x: int | None):
    if 1 + 1 == 2:
        if x is None:
            return
        reveal_type(x)  # revealed: int

    # TODO: should be `int` (the else-branch of `1 + 1 == 2` is unreachable)
    reveal_type(x)  # revealed: int | None
```

This also works when the always-true condition is nested inside a narrowing branch:

```py
def _(x: int | None):
    if x is None:
        if 1 + 1 == 2:
            return

    # TODO: should be `int` (the inner always-true branch makes the outer
    # if-branch terminal)
    reveal_type(x)  # revealed: int | None
```

## Narrowing from `assert` should not affect reassigned variables

When a variable is reassigned after an `assert`, the narrowing from the assert should not apply to
the new value.

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

## Narrowing from `NoReturn` should not affect reassigned variables

When a variable is narrowed due to a `NoReturn` call in one branch and then reassigned, the
narrowing should only apply before the reassignment, not after.

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
    # v was reassigned, so any narrowing shouldn't apply
    reveal_type(v)  # revealed: int | None
```

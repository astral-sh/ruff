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

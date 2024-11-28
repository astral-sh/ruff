# Consolidating narrowed types after if statement

## After if-else statements, narrowing has no-effect if variable is not mutated

```py
def optional_int() -> int | None: ...

x = optional_int()

if x is None:
    pass
else:
    pass

reveal_type(x)  # revealed: int | None
```

## After if-else statements, narrowing has an effect if variable is mutated

```py
def optional_int() -> int | None: ...

x = optional_int()

if x is None:
    x = 10
else:
    pass

reveal_type(x)  # revealed: int
```

## if statement without explicit else branch, act similar to empty else branch

```py
def optional_int() -> int | None: ...

x = optional_int()
y = optional_int()

if x is None:
    x = 0

if y is None:
    pass

reveal_type(x)  # revealed: int
reveal_type(y)  # revealed: int | None
```

## if-elif without explicit else branch, act similar to empty else branch

```py
def optional_int() -> int | None: ...

x = optional_int()

if x is None:
    x = 0
elif x > 50:
    x = 50

reveal_type(x)  # revealed: int
```

# If expressions

## Simple if-expression

```py
def _(flag: bool):
    x = 1 if flag else 2
    reveal_type(x)  # revealed: Literal[1, 2]
```

## If-expression with walrus operator

```py
def _(flag: bool):
    y = 0
    z = 0
    x = (y := 1) if flag else (z := 2)
    reveal_type(x)  # revealed: Literal[1, 2]
    reveal_type(y)  # revealed: Literal[0, 1]
    reveal_type(z)  # revealed: Literal[0, 2]
```

## Nested if-expression

```py
def _(flag: bool, flag2: bool):
    x = 1 if flag else 2 if flag2 else 3
    reveal_type(x)  # revealed: Literal[1, 2, 3]
```

## None

```py
def _(flag: bool):
    x = 1 if flag else None
    reveal_type(x)  # revealed: Literal[1] | None
```

## Condition with object that implements `__bool__` incorrectly

```py
class NotBoolable:
    __bool__: int = 3

# error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `NotBoolable`; its `__bool__` method isn't callable"
3 if NotBoolable() else 4
```

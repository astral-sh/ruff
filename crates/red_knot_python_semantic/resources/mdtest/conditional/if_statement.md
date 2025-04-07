# If statements

## Simple if

```py
def _(flag: bool):
    y = 1
    y = 2

    if flag:
        y = 3

    reveal_type(y)  # revealed: Literal[2, 3]
```

## Simple if-elif-else

```py
def _(flag: bool, flag2: bool):
    y = 1
    y = 2

    if flag:
        y = 3
    elif flag2:
        y = 4
    else:
        r = y
        y = 5
        s = y
    x = y

    reveal_type(x)  # revealed: Literal[3, 4, 5]

    # revealed: Literal[2]
    # error: [possibly-unresolved-reference]
    reveal_type(r)

    # revealed: Literal[5]
    # error: [possibly-unresolved-reference]
    reveal_type(s)
```

## Single symbol across if-elif-else

```py
def _(flag: bool, flag2: bool):
    if flag:
        y = 1
    elif flag2:
        y = 2
    else:
        y = 3

    reveal_type(y)  # revealed: Literal[1, 2, 3]
```

## if-elif-else without else assignment

```py
def _(flag: bool, flag2: bool):
    y = 0

    if flag:
        y = 1
    elif flag2:
        y = 2
    else:
        pass

    reveal_type(y)  # revealed: Literal[0, 1, 2]
```

## if-elif-else with intervening assignment

```py
def _(flag: bool, flag2: bool):
    y = 0

    if flag:
        y = 1
        z = 3
    elif flag2:
        y = 2
    else:
        pass

    reveal_type(y)  # revealed: Literal[0, 1, 2]
```

## Nested if statement

```py
def _(flag: bool, flag2: bool):
    y = 0

    if flag:
        if flag2:
            y = 1

    reveal_type(y)  # revealed: Literal[0, 1]
```

## if-elif without else

```py
def _(flag: bool, flag2: bool):
    y = 1
    y = 2

    if flag:
        y = 3
    elif flag2:
        y = 4

    reveal_type(y)  # revealed: Literal[2, 3, 4]
```

## if-elif with assignment expressions in tests

```py
def check(x: int) -> bool:
    return bool(x)

if check(x := 1):
    x = 2
elif check(x := 3):
    x = 4

reveal_type(x)  # revealed: Literal[2, 3, 4]
```

## constraints apply to later test expressions

```py
def check(x) -> bool:
    return bool(x)

def _(flag: bool):
    x = 1 if flag else None
    y = 0

    if x is None:
        pass
    elif check(y := x):
        pass

    reveal_type(y)  # revealed: Literal[0, 1]
```

## Condition with object that implements `__bool__` incorrectly

```py
class NotBoolable:
    __bool__: int = 3

# error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `NotBoolable`; its `__bool__` method isn't callable"
if NotBoolable():
    ...
# error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `NotBoolable`; its `__bool__` method isn't callable"
elif NotBoolable():
    ...
```

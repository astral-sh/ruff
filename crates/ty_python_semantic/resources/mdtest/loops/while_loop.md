# While loops

## Basic `while` loop

```py
def _(flag: bool):
    x = 1
    while flag:
        x = 2

    reveal_type(x)  # revealed: Literal[1, 2]
```

## `while` with `else` (no `break`)

```py
def _(flag: bool):
    x = 1
    while flag:
        x = 2
    else:
        reveal_type(x)  # revealed: Literal[1, 2]
        x = 3

    reveal_type(x)  # revealed: Literal[3]
```

## `while` with `else` (may `break`)

```py
def _(flag: bool, flag2: bool):
    x = 1
    y = 0
    while flag:
        x = 2
        if flag2:
            y = 4
            break
    else:
        y = x
        x = 3

    reveal_type(x)  # revealed: Literal[2, 3]
    reveal_type(y)  # revealed: Literal[4, 1, 2]
```

## Nested `while` loops

```py
def flag() -> bool:
    return True

x = 1

while flag():
    x = 2

    while flag():
        x = 3
        if flag():
            break
    else:
        x = 4

    if flag():
        break
else:
    x = 5

reveal_type(x)  # revealed: Literal[3, 4, 5]
```

## Boundness

Make sure that the boundness information is correctly tracked in `while` loop control flow.

### Basic `while` loop

```py
def _(flag: bool):
    while flag:
        x = 1

    # error: [possibly-unresolved-reference]
    x
```

### `while` with `else` (no `break`)

```py
def _(flag: bool):
    while flag:
        y = 1
    else:
        x = 1

    # no error, `x` is always bound
    x
    # error: [possibly-unresolved-reference]
    y
```

### `while` with `else` (may `break`)

```py
def _(flag: bool, flag2: bool):
    while flag:
        x = 1
        if flag2:
            break
    else:
        y = 1

    # error: [possibly-unresolved-reference]
    x
    # error: [possibly-unresolved-reference]
    y
```

## Condition with object that implements `__bool__` incorrectly

```py
class NotBoolable:
    __bool__: int = 3

# error: [unsupported-bool-conversion] "Boolean conversion is not supported for type `NotBoolable`"
while NotBoolable():
    ...
```

## Backwards control flow

```py
i = 0
reveal_type(i)  # revealed: Literal[0]
while i < 1_000_000:
    reveal_type(i)  # revealed: int
    i += 1
    reveal_type(i)  # revealed: int
reveal_type(i)  # revealed: int

# TODO: None of these should need to be raised to `int`. Loop control flow analysis should take the
# loop condition into account.
i = 0
reveal_type(i)  # revealed: Literal[0]
while i < 2:
    # TODO: Should be Literal[0, 1].
    reveal_type(i)  # revealed: int
    i += 1
    # TODO: Should be Literal[1, 2].
    reveal_type(i)  # revealed: int
# TODO: Should be Literal[2].
reveal_type(i)  # revealed: int
```

```py
def random() -> bool:
    raise NotImplementedError

i = 0
while True:
    reveal_type(i)  # revealed: Literal[0, 1, 2]
    if random():
        i = 1
    else:
        i = "break"
        break
    # To get here we must take the `i = 1` branch above.
    reveal_type(i)  # revealed: Literal[1]
    if random():
        i = 2
    reveal_type(i)  # revealed: Literal[1, 2]
reveal_type(i)  # revealed: Literal["break"]

i = 0
while random():
    if random():
        reveal_type(i)  # revealed: Literal[0, 1, 2, 3]
        i = 1
        reveal_type(i)  # revealed: Literal[1]
    while random():
        if random():
            reveal_type(i)  # revealed: Literal[1, 0, 2, 3]
            i = 2
            reveal_type(i)  # revealed: Literal[2]
    if random():
        reveal_type(i)  # revealed: Literal[1, 2, 0, 3]
        i = 3
        reveal_type(i)  # revealed: Literal[3]
```

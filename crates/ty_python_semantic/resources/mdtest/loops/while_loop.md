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

## Cyclic control flow

```py
def random() -> bool:
    return False

i = 0
reveal_type(i)  # revealed: Literal[0]
while random():
    i += 1
    reveal_type(i)  # revealed: int
```

A binding that didn't exist before the loop started:

```py
i = 0
while i < 1_000_000:
    if i > 0:
        loop_only += 1  # error: [possibly-unresolved-reference]
    if i == 0:
        loop_only = 0
    i += 1
reveal_type(i)  # revealed: int
# error: [possibly-unresolved-reference]
reveal_type(loop_only)  # revealed: int
```

A more complex example, where the loop condition narrows both the loop-back value and the
end-of-loop value.

```py
x = "A"
while x != "C":
    reveal_type(x)  # revealed: Literal["A", "B"]
    if random():
        x = "B"
    else:
        x = "C"
    reveal_type(x)  # revealed: Literal["B", "C"]
reveal_type(x)  # revealed: Literal["C"]
```

```py
x = "A"
while x != "E":
    reveal_type(x)  # revealed: Literal["A", "C", "D"]
    while x != "C":
        reveal_type(x)  # revealed: Literal["A", "D", "B"]
        if random():
            x = "B"
        else:
            x = "C"
        reveal_type(x)  # revealed: Literal["B", "C"]
    reveal_type(x)  # revealed: Literal["C"]
    if random():
        x = "D"
    if random():
        x = "E"
    reveal_type(x)  # revealed: Literal["C", "D", "E"]
reveal_type(x)  # revealed: Literal["E"]
```

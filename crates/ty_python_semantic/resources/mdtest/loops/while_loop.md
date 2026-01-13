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

## Walrus definitions in the condition are always evaluated

```py
while x := False:
    pass
reveal_type(x)  # revealed: Literal[False]
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

Similar, but with `break` and `continue`:

```py
x = "A"
while True:
    reveal_type(x)  # revealed: Literal["A", "C", "D"]
    while True:
        reveal_type(x)  # revealed: Literal["A", "C", "D", "B"]
        if random():
            x = "B"
            continue
        else:
            x = "C"
            break
        reveal_type(x)  # revealed: Never
    reveal_type(x)  # revealed: Literal["C"]
    if random():
        x = "D"
        continue
    if random():
        x = "E"
        break
    reveal_type(x)  # revealed: Literal["C"]
reveal_type(x)  # revealed: Literal["E"]
```

All the loop conditions above are static, so here are some non-static ones:

```py
x = "A"
while random():
    reveal_type(x)  # revealed: Literal["A", "B", "C", "D"]
    x = "B"
    if random():
        x = "C"
    if x == "C":
        continue
    reveal_type(x)  # revealed: Literal["B"]
    while random():
        reveal_type(x)  # revealed: Literal["B", "D"]
        if random():
            x = "D"
            continue
        x = "E"
        break
    reveal_type(x)  # revealed: Literal["B", "D", "E"]
    if x == "E":
        break
    reveal_type(x)  # revealed: Literal["B", "D"]
reveal_type(x)  # revealed: Literal["A", "B", "C", "D", "E"]
```

Functions and classes defined in loops count as bindings and are visible via loopback:

```py
foo = None
Bar = None
while random():
    reveal_type(foo)  # revealed: None | (def foo() -> None)
    reveal_type(Bar)  # revealed: None | <class 'Bar'>

    def foo() -> None: ...

    class Bar: ...
```

Loopback bindings are also visible to the walrus operator in the loop condition:

```py
i = 0
while (i := i + 1) < 1_000_000:
    reveal_type(i)  # revealed: int
```

"Member" (as opposed to "symbol") places are also given loopback bindings:

```py
my_dict = {}
my_dict["x"] = 0
reveal_type(my_dict["x"])  # revealed: Literal[0]
while random():
    my_dict["x"] += 1
reveal_type(my_dict["x"])  # revealed: int
```

`del` prevents bindings from reaching the loopback:

```py
# Start with an unreachable binding, to avoid triggering a semantic syntax error.
if False:
    unique_variable_name = 99
while random():
    unique_variable_name  # error: [unresolved-reference]
    unique_variable_name = 42
    del unique_variable_name
```

Swap bindings converge normally under fixpoint iteration:

```py
x = 1
y = 2
while random():
    x, y = y, x
    reveal_type(x)  # revealed: Literal[2, 1]
    reveal_type(y)  # revealed: Literal[1, 2]
```

And tuple assignments in general are inferred correctly:

```py
x = 0
while random():
    x, y = x + 1, None
    reveal_type(x)  # revealed: int
```

The minimized repro from sympy/core/numbers.py:

```py
x = 1
y = 2
while random():
    if x:
        x, y = y, x
    reveal_type(x)  # revealed: Literal[2, 1]
    reveal_type(y)  # revealed: Literal[1, 2]
```

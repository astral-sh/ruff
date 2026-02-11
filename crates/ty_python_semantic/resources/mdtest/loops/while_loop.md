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

### Basic

```py
def random() -> bool:
    return False

i = 0
reveal_type(i)  # revealed: Literal[0]
while random():
    i += 1
    reveal_type(i)  # revealed: int
reveal_type(i)  # revealed: int
```

### A binding that didn't exist before the loop started

```py
i = 0
while i < 1_000_000:
    if i > 0:
        loop_only += 1  # error: [possibly-unresolved-reference]
    if i == 0:
        loop_only = 0
    i += 1
# error: [possibly-unresolved-reference]
reveal_type(loop_only)  # revealed: int
```

### A more complex example

Here the loop condition narrows both the loop-back value and the end-of-loop value:

```py
def random() -> bool:
    return False

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

### An even more complex example

```py
def random() -> bool:
    return False

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

### `break` and `continue`

```py
def random() -> bool:
    return False

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

### Interaction between `break` and a narrowing condition

Here the loop condition forces `x` to be `False` at loop exit, because there is no `break`:

```py
def random() -> bool:
    return True

x = random()
reveal_type(x)  # revealed: bool
while x:
    pass
reveal_type(x)  # revealed: Literal[False]
```

However, we can't narrow `x` like this when there's a `break` in the loop:

```py
x = random()
while x:
    if random():
        break
reveal_type(x)  # revealed: bool
```

### Non-static loop conditions

```py
def random() -> bool:
    return False

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

### Functions and classes defined in loops count as bindings and are visible via loopback

```py
def random() -> bool:
    return False

foo = None
Bar = None
while random():
    reveal_type(foo)  # revealed: None | (def foo() -> None)
    reveal_type(Bar)  # revealed: None | <class 'Bar'>

    def foo() -> None: ...

    class Bar: ...
```

### Walrus operator assignments are visible via loopback

```py
def random() -> bool:
    return False

while random():
    # error: [possibly-unresolved-reference]
    reveal_type(y)  # revealed: Literal[1]
    x = (y := 1)
```

### Loopback bindings are visible to the walrus operator in the loop condition

```py
i = 0
while (i := i + 1) < 1_000_000:
    reveal_type(i)  # revealed: int
```

### "Member" (as opposed to "symbol") places are also given loopback bindings

```py
def random() -> bool:
    return False

my_dict = {}
my_dict["x"] = 0
reveal_type(my_dict["x"])  # revealed: Literal[0]
while random():
    my_dict["x"] += 1
reveal_type(my_dict["x"])  # revealed: int
```

### `del` prevents bindings from reaching the loopback

This `x` cannot reach the use at the top of the loop:

```py
def random() -> bool:
    return False

while random():
    x  # error: [unresolved-reference]
    x = 42
    del x
```

On the other hand, if `x` is defined before the loop, the `del` makes it a
`[possibly-unresolved-reference]`:

```py
x = 0
while random():
    x  # error: [possibly-unresolved-reference]
    x = 42
    del x
```

### `del` in a loop makes a variable possibly-unbound after the loop

```py
def random() -> bool:
    return False

x = 0
while random():
    # error: [possibly-unresolved-reference]
    del x
# error: [possibly-unresolved-reference]
x
```

### Bindings in a loop are possibly-unbound after the loop

```py
def random() -> bool:
    return False

while random():
    x = 42
# error: [possibly-unresolved-reference]
x
```

### Swap bindings converge normally under fixpoint iteration

```py
def random() -> bool:
    return False

x = 1
y = 2
while random():
    x, y = y, x
    # Note that we get correct types in the "avoid oscillations" test case below, but not here. I
    # believe the difference is that in this case the Salsa "cycle head" is the tuple on the RHS of
    # the assignment, which triggers our recursive type handling, whereas below it's `x`.
    # TODO: should be Literal[2, 1]
    reveal_type(x)  # revealed: Divergent
    # TODO: should be Literal[1, 2]
    reveal_type(y)  # revealed: Divergent
```

### Tuple assignments are inferred correctly

```py
def random() -> bool:
    return False

x = 0
while random():
    x, y = x + 1, None
    # TODO: should be int
    reveal_type(x)  # revealed: Divergent
```

### Loop bodies that are guaranteed to execute at least once

TODO: We should be able to see when a loop body is guaranteed to execute at least once. However,
Pyright and other checkers don't currently handle this case either.

```py
x = "foo"
while x != "bar":
    definitely_bound = 42
    x = "bar"
# TODO: We should see that `definitely_bound` is definitely bound.
# error: [possibly-unresolved-reference]
reveal_type(definitely_bound)  # revealed: Literal[42]
```

### Bindings in statically unreachable branches are excluded from loopback

```py
VAL = 1

x = 1
while True:
    reveal_type(x)  # revealed: Literal[1]
    if VAL - 1:
        x = 2
```

### `global` and `nonlocal` keywords in a loop

We need to make sure that the loop header definition doesn't count as a "use" prior to the
`global`/`nonlocal` declaration, or else we'll emit a false-positive semantic syntax error:

```py
x = 0

def _():
    y = 0
    def _():
        while True:
            global x
            nonlocal y
            x = 42
            y = 99
```

On the other hand, we don't want to shadow true positives:

```py
x = 0

def _():
    y = 0
    def _():
        x = 1
        y = 1
        while True:
            global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
            nonlocal y  # error: [invalid-syntax] "name `y` is used prior to nonlocal declaration"
```

### Use with loop header and also `UNBOUND` definitely visible

In `place_from_bindings_impl` we usually assert that if at least one (non-`UNBOUND`) binding is
visible, then `UNBOUND` should not be definitely-visible. That makes intuitive sense: either a
binding should shadow `UNBOUND` entirely, or if it was made in a branch then it should attach the
negated branch condition to `UNBOUND`. However, loop header bindings are an exception to this rule,
because they don't shadow prior bindings. In this example `UNBOUND` is definitely-visible, and we
need to avoid panicking:

```py
while True:
    x  # error: [possibly-unresolved-reference]
    x = 1
```

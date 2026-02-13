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
    pass
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

### Avoid oscillations

We need to avoid oscillating cycles in cases like the following, where the type of one of these loop
variables also influences the static reachability of its bindings. This case was minimized from a
real crash that came up during development checking these lines of `sympy`:
<https://github.com/sympy/sympy/blob/c2bfd65accf956576b58f0ae57bf5821a0c4ff49/sympy/core/numbers.py#L158-L166>

```py
def random() -> bool:
    return False

x = 1
y = 2
while random():
    if x:
        x, y = y, x
    reveal_type(x)  # revealed: Literal[2, 1]
    reveal_type(y)  # revealed: Literal[1, 2]
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

### `Divergent` in narrowing conditions doesn't run afoul of "monotonic widening" in cycle recovery

The following is a deceptively-simple-looking case of narrowing that was difficult to get right in
the initial implementation of cyclic control flow. We start with a non-empty linked list, and we
advance it in a loop until there's exactly one node left:

```py
class Node:
    def __init__(self, next: "Node | None" = None):
        self.next: "Node | None" = next

node = Node(Node(Node()))
while node.next is not None:
    node = node.next
reveal_type(node)  # revealed: Node
reveal_type(node.next)  # revealed: None
```

There's nothing wrong with this code, and it was minimized from [real cases] in the ecosystem. But
it's prone to false-positive `[possibly-missing-attribute]` warnings on the `node.next` accesses if
we lose track of the fact that the `node` variable is never `None`. Note that the loop condition
narrows `node.next`, not `node` itself, so that constraint needs to flow through the assignment in
the loop body, and through the loop header definition that sees that assignment, to the prior uses
of `node` in the loop condition and in the RHS of the assignment. We expect that to become a Salsa
cycle that we resolve through fixpoint iteration. That runs into two of our cycle recovery
behaviors:

1. When cycles show up in a standalone expression definition (in this case, the `while` loop
    condition), the `cycle_initial` value (`expression_cycle_initial`) is an empty map with a
    "fallback type" that reports `Divergent` for _every_ sub-expression. That even includes literal
    expressions like `42` and (in this case) `None`.
1. To avoid oscillations in cycle recovery (`Type::cycle_normalized`), we union together the type
    inferred in the previous iteration with the type inferred in the current one, as long as
    neither of them contains `Divergent`. In other words, we do "monotonic widening".

The interaction we have to worry about is getting stuck with a type that's too wide. When we try to
do narrowing in the first cycle iteration, `is not None` behaves like `is not Divergent`. If the
consequence is that we don't do any narrowing at all, then for that iteration we'll end up inferring
`Node | None` for `node`. (For completeness, we actually infer `Node | None | Divergent` because of
a nested cycle, but we strip out _that_ `Divergent` in another part of cycle recovery. The
[full chain of events here][divergent_debugging] is quite long.) In the second cycle iteration we'll
get the narrowing right and infer that `node` is of type `Node`, but then our monotonic widening
step will union `Node` with `Node | None` from the previous iteration, reproduce the same wrong
answer, and declare that to be the fixpoint. Finally we get false-positive warnings from the fact
that `Node` doesn't have a `.next` field.

So, because we do monotonic widening in cycle recovery, we need to make sure that temporarily
`Divergent` expressions in narrowing constraints don't lead to too-wide-but-not-visibly-`Divergent`
types. Instead, `Divergent` should "poison" any value we try to narrow against it, so that our cycle
recovery logic doesn't carry that result forward.

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

### Loop header definitions don't shadow member bindings

```py
class C:
    x = None

c = C()
c.x = 0

while True:
    reveal_type(c.x)  # revealed: Literal[0]
    c = C()
    break

d = [0]
d[0] = 1

while True:
    reveal_type(d[0])  # revealed: Literal[1]
    d = []
    break
```

[divergent_debugging]: https://github.com/astral-sh/ruff/pull/22794#issuecomment-3852095578
[real cases]: https://github.com/Finistere/antidote/blob/7d64ff76b7e283e5d9593ca09ea7a52b9b054957/src/antidote/_internal/localns.py#L34-L35

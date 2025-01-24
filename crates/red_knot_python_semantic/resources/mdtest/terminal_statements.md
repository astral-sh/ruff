# Terminal statements

## Introduction

Terminal statements complicate a naive control-flow analysis.

As a simple example:

```py
def f(cond: bool) -> str:
    if cond:
        x = "test"
    else:
        raise ValueError
    return x

def g(cond: bool):
    if cond:
        x = "test"
        reveal_type(x)  # revealed: Literal["test"]
    else:
        x = "unreachable"
        reveal_type(x)  # revealed: Literal["unreachable"]
        raise ValueError
    reveal_type(x)  # revealed: Literal["test"]
```

In `f`, we should be able to determine that the `else` branch ends in a terminal statement, and that
the `return` statement can only be executed when the condition is true. We should therefore consider
the reference always bound, even though `x` is only bound in the true branch.

Similarly, in `g`, we should see that the assignment of the value `"unreachable"` can never be seen
by the final `reveal_type`.

## `return` is terminal

```py
def f(cond: bool) -> str:
    if cond:
        x = "test"
    else:
        return "early"
    return x  # no possibly-unresolved-reference diagnostic!

def g(cond: bool):
    if cond:
        x = "test"
        reveal_type(x)  # revealed: Literal["test"]
    else:
        x = "unreachable"
        reveal_type(x)  # revealed: Literal["unreachable"]
        return
    reveal_type(x)  # revealed: Literal["test"]
```

## `continue` is terminal within its loop scope

```py
def f(cond: bool) -> str:
    while True:
        if cond:
            x = "test"
        else:
            continue
        return x

def g(cond: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond:
            x = "loop"
            reveal_type(x)  # revealed: Literal["loop"]
        else:
            x = "continue"
            reveal_type(x)  # revealed: Literal["continue"]
            continue
        reveal_type(x)  # revealed: Literal["loop"]
    reveal_type(x)  # revealed: Literal["before", "loop"]
```

## `break` is terminal within its loop scope

```py
def f(cond: bool) -> str:
    while True:
        if cond:
            x = "test"
        else:
            break
        return x
    return "late"

def g(cond: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond:
            x = "loop"
            reveal_type(x)  # revealed: Literal["loop"]
        else:
            x = "break"
            reveal_type(x)  # revealed: Literal["break"]
            break
        reveal_type(x)  # revealed: Literal["loop"]
    reveal_type(x)  # revealed: Literal["before", "loop", "break"]
```

## `return` is terminal in nested conditionals

```py
def f(cond1: bool, cond2: bool) -> str:
    if cond1:
        if cond2:
            x = "test1"
        else:
            return "early"
    else:
        x = "test2"
    return x

def g(cond1: bool, cond2: bool):
    if cond1:
        if cond2:
            x = "test1"
            reveal_type(x)  # revealed: Literal["test1"]
        else:
            x = "unreachable"
            reveal_type(x)  # revealed: Literal["unreachable"]
            return
        reveal_type(x)  # revealed: Literal["test1"]
    else:
        x = "test2"
        reveal_type(x)  # revealed: Literal["test2"]
    reveal_type(x)  # revealed: Literal["test1", "test2"]
```

## Terminal in a `finally` block

Control-flow through finally isn't working right yet:

```py
def f():
    x = 1
    while True:
        try:
            break
        finally:
            x = 2
    # TODO: should be Literal[2]
    reveal_type(x)  # revealed: Literal[1]
```

## Terminal statement after a list comprehension

```py
def f(x: str) -> int:
    y = [x for i in range(len(x))]
    return 4
```

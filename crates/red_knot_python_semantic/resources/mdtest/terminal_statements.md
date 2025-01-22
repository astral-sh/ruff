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
    else:
        x = "unreachable"
        raise ValueError
    reveal_type(x)  # revealed: Literal["test"]
```

In `f`, we should be able to determine that the `else` ends in a terminal statement, and that the
`return` statement can only be executed when the condition is true. Even though `x` is only bound in
the true branch, we should therefore consider the reference always bound.

Similarly, in `g`, we should see that the assignment of the value `"unreachable"` can never be seen
by the `reveal_type`.

## `return` is terminal

```py
def f(cond: bool) -> str:
    if cond:
        x = "test"
    else:
        return "early"
    return x

def g(cond: bool):
    if cond:
        x = "test"
    else:
        x = "unreachable"
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

def g(cond: bool):
    while True:
        if cond:
            x = "test"
        else:
            x = "unreachable"
            continue
        reveal_type(x)  # revealed: Literal["test"]
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

def g(cond: bool):
    while True:
        if cond:
            x = "test"
        else:
            x = "unreachable"
            break
        reveal_type(x)  # revealed: Literal["test"]
```

## `return` is terminal in nested scopes

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
        else:
            x = "unreachable"
            return
    else:
        x = "test2"
    reveal_type(x)  # revealed: Literal["test1", "test2"]
```

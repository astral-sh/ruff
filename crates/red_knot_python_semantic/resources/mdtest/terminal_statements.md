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
        x = "terminal"
        reveal_type(x)  # revealed: Literal["terminal"]
        raise ValueError
    reveal_type(x)  # revealed: Literal["test"]
```

In `f`, we should be able to determine that the `else` branch ends in a terminal statement, and that
the `return` statement can only be executed when the condition is true. We should therefore consider
the reference always bound, even though `x` is only bound in the true branch.

Similarly, in `g`, we should see that the assignment of the value `"terminal"` can never be seen by
the final `reveal_type`.

## `return`

A `return` statement is terminal; bindings that occur before it are not visible after it.

```py
def resolved_reference(cond: bool) -> str:
    if cond:
        x = "test"
    else:
        return "early"
    return x  # no possibly-unresolved-reference diagnostic!

def return_in_then_branch(cond: bool):
    if cond:
        x = "terminal"
        reveal_type(x)  # revealed: Literal["terminal"]
        return
    else:
        x = "test"
        reveal_type(x)  # revealed: Literal["test"]
    reveal_type(x)  # revealed: Literal["test"]

def return_in_else_branch(cond: bool):
    if cond:
        x = "test"
        reveal_type(x)  # revealed: Literal["test"]
    else:
        x = "terminal"
        reveal_type(x)  # revealed: Literal["terminal"]
        return
    reveal_type(x)  # revealed: Literal["test"]

def return_in_both_branches(cond: bool):
    if cond:
        x = "terminal1"
        reveal_type(x)  # revealed: Literal["terminal1"]
        return
    else:
        x = "terminal2"
        reveal_type(x)  # revealed: Literal["terminal2"]
        return

def return_in_try(cond: bool):
    x = "before"
    try:
        if cond:
            x = "test"
            return
    except:
        # TODO: Literal["before"]
        reveal_type(x)  # revealed: Literal["before", "test"]
    else:
        reveal_type(x)  # revealed: Literal["before"]
    finally:
        reveal_type(x)  # revealed: Literal["before", "test"]
    reveal_type(x)  # revealed: Literal["before", "test"]

def return_in_nested_then_branch(cond1: bool, cond2: bool):
    if cond1:
        x = "test1"
        reveal_type(x)  # revealed: Literal["test1"]
    else:
        if cond2:
            x = "terminal"
            reveal_type(x)  # revealed: Literal["terminal"]
            return
        else:
            x = "test2"
            reveal_type(x)  # revealed: Literal["test2"]
        reveal_type(x)  # revealed: Literal["test2"]
    reveal_type(x)  # revealed: Literal["test1", "test2"]

def return_in_nested_else_branch(cond1: bool, cond2: bool):
    if cond1:
        x = "test1"
        reveal_type(x)  # revealed: Literal["test1"]
    else:
        if cond2:
            x = "test2"
            reveal_type(x)  # revealed: Literal["test2"]
        else:
            x = "terminal"
            reveal_type(x)  # revealed: Literal["terminal"]
            return
        reveal_type(x)  # revealed: Literal["test2"]
    reveal_type(x)  # revealed: Literal["test1", "test2"]

def return_in_both_nested_branches(cond1: bool, cond2: bool):
    if cond1:
        x = "test"
        reveal_type(x)  # revealed: Literal["test"]
    else:
        if cond2:
            x = "terminal1"
            reveal_type(x)  # revealed: Literal["terminal1"]
            return
        else:
            x = "terminal2"
            reveal_type(x)  # revealed: Literal["terminal2"]
            return
    reveal_type(x)  # revealed: Literal["test"]
```

## `continue`

A `continue` statement jumps back to the top of the innermost loop. This makes it terminal within
the loop body: definitions before it are not visible after it within the rest of the loop body. They
are likely to visible after the loop body, since loops do not introduce new scopes. (Statically
known infinite loops are one exception — if control never leaves the loop body, bindings inside of
the loop are not visible outside of it.)

TODO: We are not currently modeling the cyclic control flow for loops, pending fixpoint support in
Salsa. The false positives in this section are because of that, and not our terminal statement
support. See [ruff#14160](https://github.com/astral-sh/ruff/issues/14160) for more details.

```py
def resolved_reference(cond: bool) -> str:
    while True:
        if cond:
            x = "test"
        else:
            continue
        return x

def continue_in_then_branch(cond: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond:
            x = "continue"
            reveal_type(x)  # revealed: Literal["continue"]
            continue
        else:
            x = "loop"
            reveal_type(x)  # revealed: Literal["loop"]
        reveal_type(x)  # revealed: Literal["loop"]
    # TODO: Should be Literal["before", "loop", "continue"]
    reveal_type(x)  # revealed: Literal["before", "loop"]

def continue_in_else_branch(cond: bool, i: int):
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
    # TODO: Should be Literal["before", "loop", "continue"]
    reveal_type(x)  # revealed: Literal["before", "loop"]

def continue_in_both_branches(cond: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond:
            x = "continue1"
            reveal_type(x)  # revealed: Literal["continue1"]
            continue
        else:
            x = "continue2"
            reveal_type(x)  # revealed: Literal["continue2"]
            continue
    # TODO: Should be Literal["before", "continue1", "continue2"]
    reveal_type(x)  # revealed: Literal["before"]

def continue_in_nested_then_branch(cond1: bool, cond2: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond1:
            x = "loop1"
            reveal_type(x)  # revealed: Literal["loop1"]
        else:
            if cond2:
                x = "continue"
                reveal_type(x)  # revealed: Literal["continue"]
                continue
            else:
                x = "loop2"
                reveal_type(x)  # revealed: Literal["loop2"]
            reveal_type(x)  # revealed: Literal["loop2"]
        reveal_type(x)  # revealed: Literal["loop1", "loop2"]
    # TODO: Should be Literal["before", "loop1", "loop2", "continue"]
    reveal_type(x)  # revealed: Literal["before", "loop1", "loop2"]

def continue_in_nested_else_branch(cond1: bool, cond2: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond1:
            x = "loop1"
            reveal_type(x)  # revealed: Literal["loop1"]
        else:
            if cond2:
                x = "loop2"
                reveal_type(x)  # revealed: Literal["loop2"]
            else:
                x = "continue"
                reveal_type(x)  # revealed: Literal["continue"]
                continue
            reveal_type(x)  # revealed: Literal["loop2"]
        reveal_type(x)  # revealed: Literal["loop1", "loop2"]
    # TODO: Should be Literal["before", "loop1", "loop2", "continue"]
    reveal_type(x)  # revealed: Literal["before", "loop1", "loop2"]

def continue_in_both_nested_branches(cond1: bool, cond2: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond1:
            x = "loop"
            reveal_type(x)  # revealed: Literal["loop"]
        else:
            if cond2:
                x = "continue1"
                reveal_type(x)  # revealed: Literal["continue1"]
                continue
            else:
                x = "continue2"
                reveal_type(x)  # revealed: Literal["continue2"]
                continue
        reveal_type(x)  # revealed: Literal["loop"]
    # TODO: Should be Literal["before", "loop", "continue1", "continue2"]
    reveal_type(x)  # revealed: Literal["before", "loop"]
```

## `break`

A `break` statement jumps to the end of the innermost loop. This makes it terminal within the loop
body: definitions before it are not visible after it within the rest of the loop body. They are
likely to visible after the loop body, since loops do not introduce new scopes. (Statically known
infinite loops are one exception — if control never leaves the loop body, bindings inside of the
loop are not visible outside of it.)

```py
def resolved_reference(cond: bool) -> str:
    while True:
        if cond:
            x = "test"
        else:
            break
        return x
    return x  # error: [unresolved-reference]

def break_in_then_branch(cond: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond:
            x = "break"
            reveal_type(x)  # revealed: Literal["break"]
            break
        else:
            x = "loop"
            reveal_type(x)  # revealed: Literal["loop"]
        reveal_type(x)  # revealed: Literal["loop"]
    reveal_type(x)  # revealed: Literal["before", "break", "loop"]

def break_in_else_branch(cond: bool, i: int):
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

def break_in_both_branches(cond: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond:
            x = "break1"
            reveal_type(x)  # revealed: Literal["break1"]
            break
        else:
            x = "break2"
            reveal_type(x)  # revealed: Literal["break2"]
            break
    reveal_type(x)  # revealed: Literal["before", "break1", "break2"]

def break_in_nested_then_branch(cond1: bool, cond2: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond1:
            x = "loop1"
            reveal_type(x)  # revealed: Literal["loop1"]
        else:
            if cond2:
                x = "break"
                reveal_type(x)  # revealed: Literal["break"]
                break
            else:
                x = "loop2"
                reveal_type(x)  # revealed: Literal["loop2"]
            reveal_type(x)  # revealed: Literal["loop2"]
        reveal_type(x)  # revealed: Literal["loop1", "loop2"]
    reveal_type(x)  # revealed: Literal["before", "loop1", "break", "loop2"]

def break_in_nested_else_branch(cond1: bool, cond2: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond1:
            x = "loop1"
            reveal_type(x)  # revealed: Literal["loop1"]
        else:
            if cond2:
                x = "loop2"
                reveal_type(x)  # revealed: Literal["loop2"]
            else:
                x = "break"
                reveal_type(x)  # revealed: Literal["break"]
                break
            reveal_type(x)  # revealed: Literal["loop2"]
        reveal_type(x)  # revealed: Literal["loop1", "loop2"]
    reveal_type(x)  # revealed: Literal["before", "loop1", "loop2", "break"]

def break_in_both_nested_branches(cond1: bool, cond2: bool, i: int):
    x = "before"
    for _ in range(i):
        if cond1:
            x = "loop"
            reveal_type(x)  # revealed: Literal["loop"]
        else:
            if cond2:
                x = "break1"
                reveal_type(x)  # revealed: Literal["break1"]
                break
            else:
                x = "break2"
                reveal_type(x)  # revealed: Literal["break2"]
                break
        reveal_type(x)  # revealed: Literal["loop"]
    reveal_type(x)  # revealed: Literal["before", "loop", "break1", "break2"]
```

## `raise`

A `raise` statement is terminal. If it occurs in a lexically containing `try` statement, it will
jump to one of the `except` clauses (if it matches the value being raised), or to the `finally`
clause (if none match). Currently, we assume definitions from before the `raise` are visible in all
`except` and `finally` clauses. (In the future, we might analyze the `except` clauses to see which
ones match the value being raised, and limit visibility to those clauses.) Definitions from before
the `raise` are not visible in any `else` clause, but are visible after the containing `try`
statement, since `try` does not introduce a new scope.

TODO: We are not currently implementing the "jump" behavior correctly for `raise` statements. The
false positives in this section are because of that, and not our terminal statement support.

```py
def raise_in_then_branch(cond: bool):
    x = "before"
    try:
        if cond:
            x = "raise"
            reveal_type(x)  # revealed: Literal["raise"]
            raise ValueError
        else:
            x = "else"
            reveal_type(x)  # revealed: Literal["else"]
        reveal_type(x)  # revealed: Literal["else"]
    except ValueError:
        # TODO: Literal["raise"]
        reveal_type(x)  # revealed: Literal["before", "raise", "else"]
    except:
        # TODO: Literal["raise"] or Never
        reveal_type(x)  # revealed: Literal["before", "raise", "else"]
    else:
        reveal_type(x)  # revealed: Literal["else"]
    finally:
        # This includes "before" because we assume that an exception might have occurred before the
        # `if` statement.
        reveal_type(x)  # revealed: Literal["before", "raise", "else"]
    reveal_type(x)  # revealed: Literal["before", "raise", "else"]

def raise_in_else_branch(cond: bool):
    x = "before"
    try:
        if cond:
            x = "else"
            reveal_type(x)  # revealed: Literal["else"]
        else:
            x = "raise"
            reveal_type(x)  # revealed: Literal["raise"]
            raise ValueError
        reveal_type(x)  # revealed: Literal["else"]
    except ValueError:
        # TODO: Literal["raise"]
        reveal_type(x)  # revealed: Literal["before", "else", "raise"]
    except:
        # TODO: Literal["raise"] or Never
        reveal_type(x)  # revealed: Literal["before", "else", "raise"]
    else:
        reveal_type(x)  # revealed: Literal["else"]
    finally:
        # This includes "before" because we assume that an exception might have occurred before the
        # `if` statement.
        reveal_type(x)  # revealed: Literal["before", "else", "raise"]
    reveal_type(x)  # revealed: Literal["before", "else", "raise"]

def raise_in_both_branches(cond: bool):
    x = "before"
    try:
        if cond:
            x = "raise1"
            reveal_type(x)  # revealed: Literal["raise1"]
            raise ValueError
        else:
            x = "raise2"
            reveal_type(x)  # revealed: Literal["raise2"]
            raise ValueError
    except ValueError:
        # TODO: Literal["raise1", "raise2"]
        reveal_type(x)  # revealed: Literal["before", "raise1", "raise2"]
    except:
        # TODO: Literal["raise1", "raise2"] or Never
        reveal_type(x)  # revealed: Literal["before", "raise1", "raise2"]
    else:
        # This is unreachable
        pass
    finally:
        # This includes "before" because we assume that an exception might have occurred before the
        # `if` statement.
        reveal_type(x)  # revealed: Literal["before", "raise1", "raise2"]
    reveal_type(x)  # revealed: Literal["before", "raise1", "raise2"]

def raise_in_nested_then_branch(cond1: bool, cond2: bool):
    x = "before"
    try:
        if cond1:
            x = "else1"
            reveal_type(x)  # revealed: Literal["else1"]
        else:
            if cond2:
                x = "raise"
                reveal_type(x)  # revealed: Literal["raise"]
                raise ValueError
            else:
                x = "else2"
                reveal_type(x)  # revealed: Literal["else2"]
            reveal_type(x)  # revealed: Literal["else2"]
        reveal_type(x)  # revealed: Literal["else1", "else2"]
    except ValueError:
        # TODO: Literal["raise"]
        reveal_type(x)  # revealed: Literal["before", "else1", "raise", "else2"]
    except:
        # TODO: Literal["raise"] or Never
        reveal_type(x)  # revealed: Literal["before", "else1", "raise", "else2"]
    else:
        reveal_type(x)  # revealed: Literal["else1", "else2"]
    finally:
        # This includes "before" because we assume that an exception might have occurred before the
        # `if` statement.
        reveal_type(x)  # revealed: Literal["before", "else1", "raise", "else2"]
    reveal_type(x)  # revealed: Literal["before", "else1", "raise", "else2"]

def raise_in_nested_else_branch(cond1: bool, cond2: bool):
    x = "before"
    try:
        if cond1:
            x = "else1"
            reveal_type(x)  # revealed: Literal["else1"]
        else:
            if cond2:
                x = "else2"
                reveal_type(x)  # revealed: Literal["else2"]
            else:
                x = "raise"
                reveal_type(x)  # revealed: Literal["raise"]
                raise ValueError
            reveal_type(x)  # revealed: Literal["else2"]
        reveal_type(x)  # revealed: Literal["else1", "else2"]
    except ValueError:
        # TODO: Literal["raise"]
        reveal_type(x)  # revealed: Literal["before", "else1", "else2", "raise"]
    except:
        # TODO: Literal["raise"] or Never
        reveal_type(x)  # revealed: Literal["before", "else1", "else2", "raise"]
    else:
        reveal_type(x)  # revealed: Literal["else1", "else2"]
    finally:
        # This includes "before" because we assume that an exception might have occurred before the
        # `if` statement.
        reveal_type(x)  # revealed: Literal["before", "else1", "else2", "raise"]
    reveal_type(x)  # revealed: Literal["before", "else1", "else2", "raise"]

def raise_in_both_nested_branches(cond1: bool, cond2: bool):
    x = "before"
    try:
        if cond1:
            x = "else"
            reveal_type(x)  # revealed: Literal["else"]
        else:
            if cond2:
                x = "raise1"
                reveal_type(x)  # revealed: Literal["raise1"]
                raise ValueError
            else:
                x = "raise2"
                reveal_type(x)  # revealed: Literal["raise2"]
                raise ValueError
        reveal_type(x)  # revealed: Literal["else"]
    except ValueError:
        # TODO: Literal["raise1", "raise2"]
        reveal_type(x)  # revealed: Literal["before", "else", "raise1", "raise2"]
    except:
        # TODO: Literal["raise1", "raise2"] or Never
        reveal_type(x)  # revealed: Literal["before", "else", "raise1", "raise2"]
    else:
        reveal_type(x)  # revealed: Literal["else"]
    finally:
        # This includes "before" because we assume that an exception might have occurred before the
        # `if` statement.
        reveal_type(x)  # revealed: Literal["before", "else", "raise1", "raise2"]
    reveal_type(x)  # revealed: Literal["before", "else", "raise1", "raise2"]
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

## Nested functions

Free references inside of a function body refer to variables defined in the containing scope.
Function bodies are _lazy scopes_: at runtime, these references are not resolved immediately at the
point of the function definition. Instead, they are resolved _at the time of the call_, which means
that their values (and types) can be different for different invocations. For simplicity, we instead
resolve free references _at the end of the containing scope_. That means that in the examples below,
all of the `x` bindings should be visible to the `reveal_type`, regardless of where we place the
`return` statements.

TODO: These currently produce the wrong results, but not because of our terminal statement support.
See [ruff#15777](https://github.com/astral-sh/ruff/issues/15777) for more details.

```py
def top_level_return(cond1: bool, cond2: bool):
    x = 1

    def g():
        # TODO eliminate Unknown
        reveal_type(x)  # revealed: Unknown | Literal[1, 2, 3]
    if cond1:
        if cond2:
            x = 2
        else:
            x = 3
    return

def return_from_if(cond1: bool, cond2: bool):
    x = 1

    def g():
        # TODO: Literal[1, 2, 3]
        reveal_type(x)  # revealed: Unknown | Literal[1]
    if cond1:
        if cond2:
            x = 2
        else:
            x = 3
        return

def return_from_nested_if(cond1: bool, cond2: bool):
    x = 1

    def g():
        # TODO: Literal[1, 2, 3]
        reveal_type(x)  # revealed: Unknown | Literal[1, 3]
    if cond1:
        if cond2:
            x = 2
            return
        else:
            x = 3
```

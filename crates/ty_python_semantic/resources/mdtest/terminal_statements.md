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
        x = "terminal0"
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
are likely visible after the loop body, since loops do not introduce new scopes. (Statically known
infinite loops are one exception — if control never leaves the loop body, bindings inside of the
loop are not visible outside of it.)

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
likely visible after the loop body, since loops do not introduce new scopes. (Statically known
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
jump to one of the `except` clauses (if it matches the value being raised), or to the `else` clause
(if none match). Currently, we assume definitions from before the `raise` are visible in all
`except` and `else` clauses. (In the future, we might analyze the `except` clauses to see which ones
match the value being raised, and limit visibility to those clauses.) Definitions from before the
`raise` are not visible in any `else` clause, but are visible in `except` clauses or after the
containing `try` statement (since control flow may have passed through an `except`).

Currently we assume that an exception could be raised anywhere within a `try` block. We may want to
implement a more precise understanding of where exceptions (barring `KeyboardInterrupt` and
`MemoryError`) can and cannot actually be raised.

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
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "raise", "else"]
    except:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "raise", "else"]
    else:
        reveal_type(x)  # revealed: Literal["else"]
    finally:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "raise", "else"]
    # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
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
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else", "raise"]
    except:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else", "raise"]
    else:
        reveal_type(x)  # revealed: Literal["else"]
    finally:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else", "raise"]
    # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
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
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "raise1", "raise2"]
    except:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "raise1", "raise2"]
    else:
        # This branch is unreachable, since all control flows in the `try` clause raise exceptions.
        # As a result, this binding should never be reachable, since new bindings are visible only
        # when they are reachable.
        x = "unreachable"
    finally:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "raise1", "raise2"]
    # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
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
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else1", "raise", "else2"]
    except:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else1", "raise", "else2"]
    else:
        reveal_type(x)  # revealed: Literal["else1", "else2"]
    finally:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else1", "raise", "else2"]
    # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
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
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else1", "else2", "raise"]
    except:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else1", "else2", "raise"]
    else:
        reveal_type(x)  # revealed: Literal["else1", "else2"]
    finally:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else1", "else2", "raise"]
    # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
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
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else", "raise1", "raise2"]
    except:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else", "raise1", "raise2"]
    else:
        reveal_type(x)  # revealed: Literal["else"]
    finally:
        # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
        reveal_type(x)  # revealed: Literal["before", "else", "raise1", "raise2"]
    # Exceptions can occur anywhere, so "before" and "raise" are valid possibilities
    reveal_type(x)  # revealed: Literal["before", "else", "raise1", "raise2"]
```

## Terminal in `try` with `finally` clause

TODO: we don't yet model that a `break` or `continue` in a `try` block will jump to a `finally`
clause before it jumps to end/start of the loop.

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

## Calls to functions returning `Never` / `NoReturn`

These calls should be treated as terminal statements.

### No implicit return

If we see a call to a function returning `Never`, we should be able to understand that the function
cannot implicitly return `None`. In the below examples, verify that there are no errors emitted for
invalid return type.

```py
from typing import NoReturn
import sys

def f() -> NoReturn:
    sys.exit(1)
```

Let's try cases where the function annotated with `NoReturn` is some sub-expression.

```py
from typing import NoReturn
import sys

# TODO: this is currently not yet supported
# error: [invalid-return-type]
def _() -> NoReturn:
    3 + sys.exit(1)

# TODO: this is currently not yet supported
# error: [invalid-return-type]
def _() -> NoReturn:
    3 if sys.exit(1) else 4
```

### Type narrowing

If a variable's type is a union, and some types in the union result in a function marked with
`NoReturn` being called, then we should correctly narrow the variable's type.

```py
from typing import NoReturn
import sys

def g(x: int | None):
    if x is None:
        sys.exit(1)

    # TODO: should be just `int`, not `int | None`
    # See https://github.com/astral-sh/ty/issues/685
    reveal_type(x)  # revealed: int | None
```

### Possibly unresolved diagnostics

If the codepath on which a variable is not defined eventually returns `Never`, use of the variable
should not give any diagnostics.

```py
import sys

def _(flag: bool):
    if flag:
        x = 3
    else:
        sys.exit()

    x  # No possibly-unresolved-references diagnostic here.
```

Similarly, there shouldn't be any diagnostics if the `except` block of a `try/except` construct has
a call with `NoReturn`.

```py
import sys

def _():
    try:
        x = 3
    except:
        sys.exit()

    x  # No possibly-unresolved-references diagnostic here.
```

### Bindings in branches

In case of a `NoReturn` call being present in conditionals, the revealed type of the end of the
branch should reflect the path which did not hit any of the `NoReturn` calls. These tests are
similar to the ones for `return` above.

```py
import sys

def call_in_then_branch(cond: bool):
    if cond:
        x = "terminal"
        reveal_type(x)  # revealed: Literal["terminal"]
        sys.exit()
    else:
        x = "test"
        reveal_type(x)  # revealed: Literal["test"]
    reveal_type(x)  # revealed: Literal["test"]

def call_in_else_branch(cond: bool):
    if cond:
        x = "test"
        reveal_type(x)  # revealed: Literal["test"]
    else:
        x = "terminal"
        reveal_type(x)  # revealed: Literal["terminal"]
        sys.exit()
    reveal_type(x)  # revealed: Literal["test"]

def call_in_both_branches(cond: bool):
    if cond:
        x = "terminal1"
        reveal_type(x)  # revealed: Literal["terminal1"]
        sys.exit()
    else:
        x = "terminal2"
        reveal_type(x)  # revealed: Literal["terminal2"]
        sys.exit()

    reveal_type(x)  # revealed: Never

def call_in_nested_then_branch(cond1: bool, cond2: bool):
    if cond1:
        x = "test1"
        reveal_type(x)  # revealed: Literal["test1"]
    else:
        if cond2:
            x = "terminal"
            reveal_type(x)  # revealed: Literal["terminal"]
            sys.exit()
        else:
            x = "test2"
            reveal_type(x)  # revealed: Literal["test2"]
        reveal_type(x)  # revealed: Literal["test2"]
    reveal_type(x)  # revealed: Literal["test1", "test2"]

def call_in_nested_else_branch(cond1: bool, cond2: bool):
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
            sys.exit()
        reveal_type(x)  # revealed: Literal["test2"]
    reveal_type(x)  # revealed: Literal["test1", "test2"]

def call_in_both_nested_branches(cond1: bool, cond2: bool):
    if cond1:
        x = "test"
        reveal_type(x)  # revealed: Literal["test"]
    else:
        x = "terminal0"
        if cond2:
            x = "terminal1"
            reveal_type(x)  # revealed: Literal["terminal1"]
            sys.exit()
        else:
            x = "terminal2"
            reveal_type(x)  # revealed: Literal["terminal2"]
            sys.exit()
    reveal_type(x)  # revealed: Literal["test"]
```

### Overloads

If only some overloads of a function are marked with `NoReturn`, we should run the overload
evaluation algorithm when evaluating the constraints.

```py
from typing import NoReturn, overload

@overload
def f(x: int) -> NoReturn: ...
@overload
def f(x: str) -> int: ...
def f(x): ...

# No errors
def _() -> NoReturn:
    f(3)

# This should be an error because of implicitly returning `None`
# error: [invalid-return-type]
def _() -> NoReturn:
    f("")
```

### Other callables

If other types of callables are annotated with `NoReturn`, we should still be ablt to infer correct
reachability.

```py
import sys

from typing import NoReturn

class C:
    def __call__(self) -> NoReturn:
        sys.exit()

    def die(self) -> NoReturn:
        sys.exit()

# No "implicitly returns `None`" diagnostic
def _() -> NoReturn:
    C()()

# No "implicitly returns `None`" diagnostic
def _() -> NoReturn:
    C().die()
```

## Nested functions

Free references inside of a function body refer to variables defined in the containing scope.
Function bodies are _lazy scopes_: at runtime, these references are not resolved immediately at the
point of the function definition. Instead, they are resolved _at the time of the call_, which means
that their values (and types) can be different for different invocations. For simplicity, we
currently consider _all reachable bindings_ in the containing scope:

```py
def top_level_return(cond1: bool, cond2: bool):
    x = 1

    def g():
        reveal_type(x)  # revealed: Literal[1, 2, 3]
    if cond1:
        if cond2:
            x = 2
        else:
            x = 3
    return

def return_from_if(cond1: bool, cond2: bool):
    x = 1

    def g():
        reveal_type(x)  # revealed: Literal[1, 2, 3]
    if cond1:
        if cond2:
            x = 2
        else:
            x = 3
        return

def return_from_nested_if(cond1: bool, cond2: bool):
    x = 1

    def g():
        reveal_type(x)  # revealed: Literal[1, 2, 3]
    if cond1:
        if cond2:
            x = 2
            return
        else:
            x = 3
```

## Statically known terminal statements

We model reachability using the same constraints that we use to model statically known bounds. In
this example, we see that the `return` statement is always executed, and therefore that the `"b"`
assignment is not visible to the `reveal_type`.

```py
def _(cond: bool):
    x = "a"
    if cond:
        x = "b"
        if True:
            return

    reveal_type(x)  # revealed: Literal["a"]
```

## Bindings after a terminal statement are unreachable

Any bindings introduced after a terminal statement are unreachable, and are currently considered not
visible. We [anticipate](https://github.com/astral-sh/ruff/issues/15797) that we want to provide a
more useful analysis for code after terminal statements.

```py
def f(cond: bool) -> str:
    x = "before"
    if cond:
        reveal_type(x)  # revealed: Literal["before"]
        return "a"
        x = "after-return"
        reveal_type(x)  # revealed: Never
    else:
        x = "else"
    return reveal_type(x)  # revealed: Literal["else"]
```

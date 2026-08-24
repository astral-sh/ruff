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
        if cond is True:
            x = "test"
            return
    except:
        reveal_type(x)  # revealed: Never
    else:
        reveal_type(x)  # revealed: Literal["before"]
    finally:
        # TODO: should include `Literal["test"]` when the return passes through `finally`
        # https://github.com/astral-sh/ty/issues/233
        reveal_type(x)  # revealed: Literal["before"]
    reveal_type(x)  # revealed: Literal["before"]

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
    reveal_type(x)  # revealed: Literal["before", "continue", "loop"]

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
    reveal_type(x)  # revealed: Literal["before", "loop", "continue"]

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
    reveal_type(x)  # revealed: Literal["before", "continue1", "continue2"]

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
    reveal_type(x)  # revealed: Literal["before", "loop1", "continue", "loop2"]

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
    reveal_type(x)  # revealed: Literal["before", "loop1", "loop2", "continue"]

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
    reveal_type(x)  # revealed: Literal["before", "loop", "continue1", "continue2"]
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

A `raise` statement is terminal. Inside a `try` statement, it jumps to a matching `except` clause or
propagates out of the statement. We do not yet determine which typed handler matches the exception,
so every handler sees the same possible values.

When only one branch raises, the exception handler sees only the value assigned in that branch:

```py
def raise_in_then_branch(cond: bool):
    x = "before"
    try:
        if cond is True:
            x = "raise"
            raise ValueError
        x = "else"
    except ValueError:
        reveal_type(x)  # revealed: Literal["raise"]
    else:
        reveal_type(x)  # revealed: Literal["else"]
    reveal_type(x)  # revealed: Literal["raise", "else"]
```

If both branches raise, the handler sees either value and the `else` clause cannot run:

```py
def raise_in_both_branches(cond: bool):
    x = "before"
    try:
        if cond is True:
            x = "raise1"
            raise ValueError
        else:
            x = "raise2"
            raise ValueError
    except ValueError:
        reveal_type(x)  # revealed: Literal["raise1", "raise2"]
    else:
        x = "unreachable"
    reveal_type(x)  # revealed: Literal["raise1", "raise2"]
```

Nested conditions do not make values from non-raising branches visible to the exception handler:

```py
def raise_in_nested_branch(cond1: bool, cond2: bool):
    x = "before"
    try:
        if cond1 is True:
            x = "else1"
        elif cond2 is True:
            x = "raise"
            raise ValueError
        else:
            x = "else2"
    except ValueError:
        reveal_type(x)  # revealed: Literal["raise"]
    else:
        reveal_type(x)  # revealed: Literal["else1", "else2"]
    reveal_type(x)  # revealed: Literal["else1", "raise", "else2"]
```

Multiple raising branches inside a nested condition remain visible to the handler:

```py
def raise_in_both_nested_branches(cond1: bool, cond2: bool):
    x = "before"
    try:
        if cond1 is True:
            x = "else"
        elif cond2 is True:
            x = "raise1"
            raise ValueError
        else:
            x = "raise2"
            raise ValueError
    except ValueError:
        reveal_type(x)  # revealed: Literal["raise1", "raise2"]
    else:
        reveal_type(x)  # revealed: Literal["else"]
    reveal_type(x)  # revealed: Literal["else", "raise1", "raise2"]
```

## Terminal in `try` with `finally` clause

We model terminal control flow in a `try`, `except`, or `else` block as jumping to a `finally`
clause before it terminates the current scope or jumps to its final destination when there are no
normal paths into the `finally` block.

TODO: we don't yet consider both normal and terminal entry states when checking a `finally` block
that has a mix of normal and terminal entry paths. See
[ty#233](https://github.com/astral-sh/ty/issues/233).

```py
def finally_runs_after_return():
    x = "before"
    try:
        x = "return"
        return
    finally:
        reveal_type(x)  # revealed: Literal["return"]

def finally_runs_after_try_and_except_are_terminal(cond: bool):
    x = "before"
    try:
        if cond:
            x = "try-return"
            return
        else:
            x = "try-raise"
            raise ValueError
    except ValueError:
        x = "except-return"
        return
    finally:
        reveal_type(x)  # revealed: Literal["try-return", "try-raise", "except-return"]

def finally_runs_after_except_and_else_are_terminal():
    x = "before"
    try:
        x = "try-normal"
    except ValueError:
        x = "except-return"
        return
    else:
        x = "else-return"
        return
    finally:
        reveal_type(x)  # revealed: Literal["else-return"]

def finally_runs_after_mixed_except_paths(cond: bool):
    x = "before"
    try:
        raise ValueError
    except ValueError:
        if cond:
            x = "except-return"
            return
        x = "except-normal"
    finally:
        # TODO: should also include `Literal["except-return"]`
        reveal_type(x)  # revealed: Literal["except-normal"]

def finally_runs_after_mixed_try_paths(cond: bool):
    x = "before"
    try:
        if cond:
            x = "try-return"
            return
        x = "try-normal"
    finally:
        # TODO: should also include `Literal["try-return"]`
        reveal_type(x)  # revealed: Literal["try-normal"]

def finally_runs_after_mixed_break_paths(cond: bool):
    x = "before"
    while True:
        try:
            if cond:
                x = "break"
                break
            x = "normal"
        finally:
            # TODO: should also include `Literal["break"]`
            reveal_type(x)  # revealed: Literal["normal"]
        break

def finally_runs_before_break():
    x = "before"
    while True:
        try:
            x = "break"
            break
        finally:
            reveal_type(x)  # revealed: Literal["break"]

def finally_runs_before_continue(cond: bool):
    while cond:
        x = "before"
        try:
            x = "continue"
            continue
        finally:
            reveal_type(x)  # revealed: Literal["continue"]

def nested_finally_runs_after_return():
    x = "before"
    try:
        try:
            x = "return"
            return
        finally:
            reveal_type(x)  # revealed: Literal["return"]
    finally:
        reveal_type(x)  # revealed: Literal["return"]

def nested_outer_finally_sees_inner_finally_assignments():
    x = "before"
    try:
        try:
            x = "return"
            return
        finally:
            x = "inner-finally"
    finally:
        reveal_type(x)  # revealed: Literal["inner-finally"]

def finally_assignment_runs_before_break():
    x = 1
    while True:
        try:
            break
        finally:
            x = 2
    # TODO: should be Literal[2]
    reveal_type(x)  # revealed: Literal[1]
```

## Returning from a context manager inside `try`

A context manager cannot prevent a `return` from reaching the enclosing `finally` block. The block
still sees assignments made before the return.

```py
from contextlib import suppress

def returns_through_finally() -> None:
    value = "before"
    try:
        with suppress(ValueError):
            value = "returned"
            return
    finally:
        reveal_type(value)  # revealed: Literal["returned"]
```

## Continuing after a suppressing context manager inside `try`

When a context manager suppresses an exception, a later assignment determines the value observed by
the `finally` block:

```py
from contextlib import suppress

value = "before"
try:
    with suppress(ValueError):
        raise ValueError
    value = "continuing"
finally:
    reveal_type(value)  # revealed: Literal["continuing"]
```

## Continuing after a suppressing context manager and `finally`

After an exception is suppressed, assignments in the `finally` block remain visible on the
continuing path:

```py
from contextlib import suppress

def continues_after_finally() -> str:
    try:
        with suppress(ValueError):
            raise ValueError
    finally:
        value = "cleanup"
    reveal_type(value)  # revealed: Literal["cleanup"]
    return value
```

## Raising from a context manager inside `try`

A `finally` block remains reachable when a context manager propagates an exception:

```py
from contextlib import nullcontext

try:
    with nullcontext():
        raise ValueError
finally:
    # The diagnostic confirms that `finally` is reachable.
    missing_name  # error: [unresolved-reference]
```

## Unreachable bindings after a context manager inside `try`

Assignments and imports after the propagating context manager cannot make the `finally` block
unreachable:

```py
from contextlib import nullcontext

try:
    with nullcontext():
        raise ValueError
    unreachable = 1
    import sys
finally:
    # The diagnostic confirms that `finally` is reachable.
    missing_after_unreachable_bindings  # error: [unresolved-reference]
```

## Code after a terminal context manager and `finally`

A non-suppressing manager does not allow a raised exception to continue past `finally` or implicitly
return from an annotated function:

```py
from contextlib import nullcontext

def does_not_continue() -> int:
    try:
        with nullcontext():
            raise ValueError
    finally:
        pass
    # The absence of a diagnostic confirms that this code is unreachable.
    missing_after_finally
```

## Narrowing after a terminal context manager and `finally`

A branch that raises through a non-suppressing manager remains terminal after its cleanup:

```py
from contextlib import nullcontext

def narrows_after_finally(value: str | None) -> None:
    if value is None:
        try:
            with nullcontext():
                raise ValueError
        finally:
            pass
    reveal_type(value)  # revealed: str
```

## Loop control after a terminal context manager and `finally`

A `break` through a non-suppressing manager and its enclosing cleanup cannot reach a later
assignment in the loop:

```py
from contextlib import nullcontext

for _ in [1]:
    try:
        with nullcontext():
            break
    finally:
        pass
    after_break = 1

after_break  # error: [unresolved-reference]
```

The same applies to `continue`:

```py
for _ in [1]:
    try:
        with nullcontext():
            continue
    finally:
        pass
    after_continue = 1

after_continue  # error: [unresolved-reference]
```

## Nested `finally` suites after a terminal context manager

The outer cleanup observes assignments made in the inner cleanup, but execution does not continue
after either suite:

```py
from contextlib import nullcontext

def nested_cleanup() -> None:
    try:
        try:
            with nullcontext():
                raise ValueError
        finally:
            value = "cleanup"
    finally:
        reveal_type(value)  # revealed: Literal["cleanup"]
    # The absence of a diagnostic confirms that this code is unreachable.
    missing_after_nested_finally
```

## Terminal `except` branches after a context manager

An `except` branch that assigns a value before returning still contributes that value to the
`finally` block:

```py
from contextlib import nullcontext

def unknown_exception() -> Exception:
    return ValueError()

def handler_returns() -> None:
    value = "before"
    try:
        with nullcontext():
            raise unknown_exception()
    except ValueError:
        value = "returned"
        return
    finally:
        reveal_type(value)  # revealed: Literal["before", "returned"]
```

## Named `except` branches after a context manager

Binding an exception does not make a terminal `except` branch a continuing entry into `finally`:

```py
from contextlib import nullcontext

def unknown_exception() -> Exception:
    return ValueError()

def named_handler() -> None:
    value = "before"
    try:
        with nullcontext():
            raise unknown_exception()
    except ValueError as error:
        value = error
        return
    finally:
        reveal_type(value)  # revealed: Literal["before"] | ValueError
```

## Multiple terminal `except` branches after a context manager

Every terminal `except` branch contributes its assignment to the `finally` block:

```py
from contextlib import nullcontext

def unknown_exception() -> Exception:
    return ValueError()

def multiple_handlers() -> None:
    value = "before"
    try:
        with nullcontext():
            raise unknown_exception()
    except ValueError:
        value = "value-error"
        return
    except TypeError:
        value = "type-error"
        raise RuntimeError
    finally:
        reveal_type(value)  # revealed: Literal["before", "value-error", "type-error"]
```

## `except` branches without terminal statements after a context manager

An `except` branch with no terminal statements determines the value observed by `finally`:

```py
from contextlib import nullcontext

value = "before"
try:
    with nullcontext():
        raise ValueError
except ValueError:
    value = "continuing"
finally:
    reveal_type(value)  # revealed: Literal["continuing"]
```

## Unreachable assignments after a context manager inside `except`

A context manager propagates an exception from an `except` branch even when an unreachable
assignment follows:

```py
from contextlib import nullcontext

try:
    raise ValueError
except ValueError:
    with nullcontext():
        raise RuntimeError
    unreachable = 1
finally:
    # The diagnostic confirms that `finally` is reachable.
    missing_after_unreachable_handler_assignment  # error: [unresolved-reference]
```

## Raising from a context manager inside a named `except` branch

Clearing a named exception does not hide the terminal path from `finally`:

```py
from contextlib import nullcontext

try:
    raise ValueError
except ValueError as error:
    with nullcontext():
        raise RuntimeError
finally:
    # The diagnostic confirms that `finally` is reachable.
    missing_name  # error: [unresolved-reference]
```

## Terminal nested `except` branches without their own `finally`

An unreachable assignment and a binding in the terminal inner `except` branch do not prevent the
path from reaching the outer `finally` block:

```py
from contextlib import nullcontext

def nested_unreachable_assignment() -> None:
    try:
        try:
            with nullcontext():
                raise ValueError
            unreachable = 1
        except ValueError:
            local = 1
            return
    finally:
        # The diagnostic confirms that `finally` is reachable.
        missing_after_nested_unreachable_assignment  # error: [unresolved-reference]
```

A `break` through an inner `except` branch also reaches the outer `finally` block:

```py
for _ in [1]:
    try:
        try:
            with nullcontext():
                raise ValueError
        except ValueError:
            break
    finally:
        # The diagnostic confirms that `finally` is reachable.
        missing_name  # error: [unresolved-reference]
```

## Unreachable assignments after a context manager inside `else`

A context manager propagates an exception from `else` even when an unreachable assignment follows:

```py
from contextlib import nullcontext

try:
    pass
except ValueError:
    pass
else:
    with nullcontext():
        raise RuntimeError
    unreachable = 1
finally:
    # The diagnostic confirms that `finally` is reachable.
    missing_after_unreachable_else_assignment  # error: [unresolved-reference]
```

## Possibly unbound names in `finally` after a context manager

When an assignment raises before binding a name, a `finally` block can observe that the name is
undefined:

```py
from contextlib import nullcontext

def may_raise() -> str:
    raise RuntimeError

def without_context_manager() -> str | None:
    try:
        value = may_raise()
        return may_raise()
    except ValueError:
        return None
    finally:
        # error: [possibly-unresolved-reference]
        reveal_type(value)  # revealed: str
```

A non-suppressing context manager does not prevent the `finally` block from observing that the name
may remain undefined.

```py
def with_context_manager() -> str | None:
    try:
        value = may_raise()
        with nullcontext():
            return may_raise()
    except ValueError:
        return None
    finally:
        # error: [possibly-unresolved-reference]
        reveal_type(value)  # revealed: str
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

    reveal_type(x)  # revealed: int
```

### Module scope

A terminal call at module scope removes a binding from its branch even when the branch condition
does not narrow that binding.

```py
from typing import NoReturn

def stop() -> NoReturn:
    raise RuntimeError

def continue_normally() -> None:
    pass

flag: bool = bool(input())
value = 1

if flag:
    value = "unreachable"
    stop()

reveal_type(value)  # revealed: Literal[1]
```

A call that returns normally must retain the binding from its branch.

```py
continuing_value = 1
other_flag: bool = bool(input())

if other_flag:
    continuing_value = "reachable"
    continue_normally()

reveal_type(continuing_value)  # revealed: Literal[1, "reachable"]
```

An unconditional terminal call eliminates the remaining bindings.

```py
stop()
reveal_type(continuing_value)  # revealed: Never
```

### Class scope

Terminal and non-terminal calls have the same effect on class-body bindings as they do on
module-level bindings.

```py
from typing import NoReturn

def stop() -> NoReturn:
    raise RuntimeError

def continue_normally() -> None:
    pass

flag: bool = bool(input())
other_flag: bool = bool(input())

class Example:
    value = 1

    if flag:
        value = "unreachable"
        stop()

    reveal_type(value)  # revealed: Literal[1]

    continuing_value = 1

    if other_flag:
        continuing_value = "reachable"
        continue_normally()

    reveal_type(continuing_value)  # revealed: Literal[1, "reachable"]

    stop()
    reveal_type(continuing_value)  # revealed: Never
```

### Statically known branches in module and class scopes

A terminal call in the reachable branch of a statically known condition removes its module-level
binding, even though the condition does not directly narrow that binding.

```py
from typing import NoReturn

def stop() -> NoReturn:
    raise RuntimeError

flag: bool = bool(input())
module_value = 1

if flag:
    module_value = "unreachable"

    if 1 + 1 == 2:
        stop()
    else:
        pass

reveal_type(module_value)  # revealed: Literal[1]
module_value.bit_count()
```

The same statically known condition also removes the unreachable binding from a class body.

```py
class Example:
    value = 1

    if flag:
        value = "unreachable"

        if 1 + 1 == 2:
            stop()
        else:
            pass

    reveal_type(value)  # revealed: Literal[1]
    value.bit_count()
```

### Generic calls in module and class scopes

A generic call is terminal when its argument specializes the return type to `Never`.

```py
from typing import NoReturn, TypeVar, cast

T = TypeVar("T")

def identity(argument: T) -> T:
    return argument

def stop() -> NoReturn:
    raise RuntimeError

module_value = 1

if bool(input()):
    module_value = "unreachable"
    identity(stop())

reveal_type(module_value)  # revealed: Literal[1]
```

Generic specialization also works when its terminal argument is not a simple call.

```py
cast_value = 1

if bool(input()):
    cast_value = "unreachable"
    identity(cast(NoReturn, None))

reveal_type(cast_value)  # revealed: Literal[1]
```

The same terminal call also narrows bindings in a class body.

```py
class Example:
    value = 1

    if bool(input()):
        value = "unreachable"
        identity(stop())

    reveal_type(value)  # revealed: Literal[1]
```

### Overloads in module scope

When only one overload returns `Never`, select the matching overload before deciding whether its
branch terminates.

```py
from typing import NoReturn, overload

@overload
def stop_if_int(argument: int) -> NoReturn: ...
@overload
def stop_if_int(argument: str) -> int: ...
def stop_if_int(argument: int | str) -> int:
    if isinstance(argument, int):
        raise RuntimeError
    return 1

flag: bool = bool(input())
value = 1

if flag:
    value = "unreachable"
    stop_if_int(1)

reveal_type(value)  # revealed: Literal[1]
```

A local argument that requires inference must still select its terminal overload.

```py
local_argument: int = int(input())
local_value = 1

if bool(input()):
    local_value = "unreachable"
    stop_if_int(local_argument)

reveal_type(local_value)  # revealed: Literal[1]
```

The overload that returns normally must not remove its branch.

```py
other_flag: bool = bool(input())
continuing_value = 1

if other_flag:
    continuing_value = "reachable"
    stop_if_int("safe")

reveal_type(continuing_value)  # revealed: Literal[1, "reachable"]
```

### Calls with no applicable bound overloads

A bound method with no applicable overloads is invalid, but its call can still return at runtime. It
must not hide bindings from its branch or subsequent diagnostics.

```py
from __future__ import annotations
from typing import overload

class Example:
    @overload
    def method(self: str) -> None: ...
    @overload
    def method(self: bytes) -> None: ...
    def method(self: Example | str | bytes) -> None:
        pass

value = 1

if bool(input()):
    value = "reachable"
    Example().method()  # error: [no-matching-overload]

reveal_type(value)  # revealed: Literal[1, "reachable"]
value.bit_count()  # error: [unresolved-attribute]
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

### Generic functions

If a generic function's return type depends on a type variable, and the argument passed resolves
that type variable to `Never`, the call should still be treated as terminal.

```py
from typing import TypeVar, NoReturn

T = TypeVar("T")

def identity(x: T) -> T:
    return x

# No "implicitly returns `None`" diagnostic
def _() -> NoReturn:
    identity(exit())

def _(flag: bool):
    if flag:
        x = "test"
    else:
        x = "terminal"
        identity(exit())

    reveal_type(x)  # revealed: Literal["test"]
```

### Other callables

If other types of callables are annotated with `NoReturn`, we should still be able to infer correct
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

### Awaiting async `NoReturn` functions

Awaiting an async function annotated as returning `NoReturn` should be treated as terminal, just
like calling a synchronous `NoReturn` function.

```py
from typing import NoReturn

async def stop() -> NoReturn:
    raise NotImplementedError

async def main(flag: bool):
    if flag:
        x = "terminal"
        await stop()
    else:
        x = "test"
        pass

    reveal_type(x)  # revealed: Literal["test"]
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

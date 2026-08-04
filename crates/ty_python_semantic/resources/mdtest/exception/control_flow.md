# Control flow for exception handlers

These tests assert that we understand the possible "definition states" (which symbols might or might
not be defined) in the various branches of a `try`/`except`/`else`/`finally` block.

For a full writeup on the semantics of exception handlers, see [this document][1].

The tests throughout this Markdown document use functions with names starting with `could_raise_*`
to mark definitions that might or might not succeed (as the function could raise an exception). A
type checker must assume that any arbitrary function call could raise an exception in Python; this
is just a naming convention used in these tests for clarity.

## Operations that cannot raise

Exception handlers are reachable only from operations that can raise. A local assignment of a
literal cannot raise, so it does not make the handler reachable:

```py
x = 1
try:
    x = 2
except:
    x = "unreachable"

reveal_type(x)  # revealed: Literal[2]
```

Truth-testing literals, identity comparisons, and Boolean combinations of known-safe expressions
cannot raise either:

```py
def known_safe_conditions(value: int | None) -> None:
    state = 0
    try:
        if not False:
            state = 1
        if not (value is None):
            state = 1
        if True and True:
            state = 1
        if False or True:
            state = 1
        for _ in [0]:
            state = 1
    except:
        state = 2

    reveal_type(state)  # revealed: Literal[1]
```

## Calls preserve completed argument evaluation

A checkpoint is recorded immediately before the raising operation, after evaluating its child
expressions. If the outer call below raises, the assignment expression has therefore completed:

```py
def may_raise(value: object) -> None: ...

x = 0
try:
    may_raise(x := 1)
except:
    reveal_type(x)  # revealed: Literal[1]
```

## Imports preserve completed aliases

Imports also checkpoint between aliases because a later import can fail after an earlier name has
been bound:

```py
first = 0
try:
    from collections.abc import Awaitable as first, Iterable as second
except:
    reveal_type(first)  # revealed: Literal[0] | <class 'Awaitable'>
```

## Explicit raises and failing assertions

Explicit raises and failing assertions also create checkpoints after their child expressions have
been evaluated:

```py
x = 1
try:
    x = 2
    raise RuntimeError
except:
    reveal_type(x)  # revealed: Literal[2]

def check_assertion(x: int | None) -> None:
    try:
        assert x is not None
    except:
        reveal_type(x)  # revealed: None

def check_short_circuit_assertion(flag: bool) -> None:
    state = 2
    try:
        assert flag and (state := 0)
    except:
        reveal_type(state)  # revealed: Literal[2, 0]
```

## Attribute access, subscripting, and operators can raise

Operations implemented by Python protocols create checkpoints after their operands have been
evaluated:

```py
class C:
    value: int

class Number:
    def __truediv__(self, other: int) -> int:
        raise NotImplementedError

def protocol_operations(c: C, number: Number, values: list[int]) -> None:
    state: C | int = 0
    try:
        (state := c).value
    except:
        reveal_type(state)  # revealed: C

    state = 0
    try:
        values[state := 1]
    except:
        reveal_type(state)  # revealed: Literal[1]

    state = 0
    try:
        number / (state := 1)
    except:
        reveal_type(state)  # revealed: Literal[1]

    state = 0
    try:
        0 < (state := 1)
    except:
        reveal_type(state)  # revealed: Literal[1]

def augmented_assignment(values: list[int]) -> None:
    target_state = 0
    rhs_state = 0
    try:
        values[target_state := 1] += (rhs_state := 1)
    except:
        reveal_type(target_state)  # revealed: Literal[1]
        reveal_type(rhs_state)  # revealed: Literal[0, 1]
```

## Truth testing, iteration, and unpacking can raise

Truth testing and iteration use Python protocols that can raise before a loop body runs or after
earlier iterations have completed. Assigning an iteration target and unpacking can raise too.

```py
from collections.abc import AsyncIterable, Iterable

class C:
    value: int

def truthiness(value: object) -> None:
    state = 0
    try:
        if value:
            state = 1
    except:
        reveal_type(state)  # revealed: Literal[0]

    state = 0
    try:
        while value:
            state = 1
    except:
        reveal_type(state)  # revealed: Literal[0, 1]

def iteration(values: Iterable[int], target: C) -> None:
    state = 0
    try:
        for _ in values:
            state = 1
    except:
        # Iteration can fail before the first item or after a completed iteration.
        reveal_type(state)  # revealed: Literal[0, 1]

    state = 0
    try:
        for target.value in [0, 1]:
            state = 1
    except:
        # Assigning the target can likewise fail on the first or a later iteration.
        reveal_type(state)  # revealed: Literal[0, 1]

async def async_iteration(values: AsyncIterable[int]) -> None:
    state = 0
    try:
        async for _ in values:
            state = 1
    except:
        reveal_type(state)  # revealed: Literal[0, 1]

def unpacking(values: Iterable[int]) -> None:
    state = 0
    try:
        first, second = values
        state = 1
    except:
        reveal_type(state)  # revealed: Literal[0]
```

## Await and yield can raise

Awaiting and yielding can raise when a coroutine or generator resumes.

```py
from collections.abc import Awaitable, Iterable

async def awaiting(value: Awaitable[int]) -> None:
    state = 0
    try:
        await value
    except:
        reveal_type(state)  # revealed: Literal[0]

def yielding_from(values: Iterable[int]):
    state = 0
    try:
        yield from values
    except:
        reveal_type(state)  # revealed: Literal[0]

def yielding():
    state = 0
    try:
        yield
    except:
        reveal_type(state)  # revealed: Literal[0]
```

## Eager and lazy scopes

Checkpoints propagate through eagerly evaluated nested scopes, but not through lazy generator
expression bodies:

```py
def may_raise() -> None: ...

x = 0
try:
    class C:
        may_raise()

except:
    x = 1

reveal_type(x)  # revealed: Literal[0, 1]

y = 0
try:
    [may_raise() for _ in [0]]
except:
    y = 1

reveal_type(y)  # revealed: Literal[0, 1]

z = 0
try:
    (may_raise() for _ in [0])
except:
    z = 1

reveal_type(z)  # revealed: Literal[0]
```

## Comprehension assignment expressions at exception checkpoints

Assignment expressions in an eager comprehension are visible to an enclosing exception handler only
after they have actually executed.

```py
from collections.abc import AsyncIterable, Awaitable

def comprehension_may_raise() -> None: ...
def comprehension_walrus_exception() -> None:
    state = 0
    try:
        [(state := 1, comprehension_may_raise()) for _ in [0]]
    except:
        reveal_type(state)  # revealed: int

def comprehension_exception_before_walrus() -> None:
    state = 0
    try:
        [(comprehension_may_raise(), state := 1) for _ in [0]]
    except:
        reveal_type(state)  # revealed: Literal[0]

def comprehension_exception_before_later_walrus() -> None:
    state = 0
    try:
        [(state := 1, comprehension_may_raise(), state := "later") for _ in [0]]
    except:
        reveal_type(state)  # revealed: int
```

Conditional assignments and multiple assignment targets retain their independent states at each
checkpoint.

```py
def comprehension_exception_after_conditional_walrus(flag: bool) -> None:
    state = "before"
    try:
        [((state := 1) if flag else 0, comprehension_may_raise(), state := 2) for _ in [0]]
    except:
        reveal_type(state)  # revealed: Literal["before"] | int

def comprehension_exception_with_multiple_walruses() -> None:
    first = 0
    second = 0
    try:
        [(first := 1, second := "bound", comprehension_may_raise(), second := 2) for _ in [0]]
    except:
        reveal_type(first)  # revealed: int
        reveal_type(second)  # revealed: str
```

Nested comprehensions apply assignments in execution order, so the inner assignment replaces the
earlier outer assignment.

```py
def nested_comprehension_walrus_order() -> None:
    state = 0
    try:
        [((state := "outer"), [(state := 1, comprehension_may_raise()) for _ in [0]]) for _ in [0]]
    except:
        reveal_type(state)  # revealed: int
```

Assignment expressions also preserve the correct module or explicitly global owning scope.

```py
module_comprehension_state = "before"
try:
    [(module_comprehension_state := 1, comprehension_may_raise()) for _ in [0]]
except:
    reveal_type(module_comprehension_state)  # revealed: int

global_comprehension_state = "before"

def global_comprehension_walrus_exception() -> None:
    global global_comprehension_state
    try:
        [(global_comprehension_state := 1, comprehension_may_raise()) for _ in [0]]
    except:
        reveal_type(global_comprehension_state)  # revealed: Literal["before"] | int
```

Async comprehensions preserve assignments that occurred before awaiting, while also accounting for
exceptions raised by iteration before the assignment executes.

```py
async def async_comprehension_walrus_exception(values: AsyncIterable[int], awaitable: Awaitable[int]) -> None:
    state = "before"
    try:
        [(state := 1, await awaitable) async for _ in values]
    except:
        reveal_type(state)  # revealed: Literal["before"] | int
```

## Nested handlers and bare-handler barriers

An active bare handler receives a checkpoint and prevents it from propagating to an enclosing
handler. Once execution is inside that handler, however, a new checkpoint can propagate outward:

```py
def may_raise() -> None: ...

x = 0
try:
    try:
        x = 1
        may_raise()
    except:
        x = 2
except:
    x = "outer"

reveal_type(x)  # revealed: Literal[1, 2]

try:
    try:
        may_raise()
    except:
        x = 3
        may_raise()
except:
    reveal_type(x)  # revealed: Literal[3]

z = 0
try:
    class C:
        try:
            pass
        except:
            may_raise()

except:
    z = 1

reveal_type(z)  # revealed: Literal[0]
```

## A single bare `except`

Consider the following `try`/`except` block, with a single bare `except:`. There are different types
for the variable `x` in the two branches of this block, and we can't determine which branch might
have been taken from the perspective of code following this block. The inferred type after the
block's conclusion is therefore the union of the type at the end of the `try` suite (`str`) and the
type at the end of the `except` suite (`Literal[2]`).

*Within* the `except` suite, we infer a union of the definition states at each exception checkpoint
in the `try` suite. The type of `x` at the beginning of the `except` suite in this example is
therefore `Literal[1] | str`: the call on the right-hand side can raise before the redefinition
completes, while the later `reveal_type` call can raise after it completes.

```py
def could_raise_returns_str() -> str:
    return "foo"

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except:
    reveal_type(x)  # revealed: Literal[1] | str
    x = 2
    reveal_type(x)  # revealed: Literal[2]

reveal_type(x)  # revealed: str | Literal[2]
```

If `x` has the same type at the end of both branches, however, the branches unify and `x` is not
inferred as having a union type following the `try`/`except` block:

```py
x = 1

try:
    x = could_raise_returns_str()
except:
    x = could_raise_returns_str()

reveal_type(x)  # revealed: str
```

## A non-bare `except`

For simple `try`/`except` blocks, an `except TypeError:` handler has the same control flow semantics
as an `except:` handler. An `except TypeError:` handler will not catch *all* exceptions: if this is
the only handler, it opens up the possibility that an exception might occur that would not be
handled. However, as described in [the document on exception-handling semantics][1], that would lead
to termination of the scope. It's therefore irrelevant to consider this possibility when it comes to
control-flow analysis.

```py
def could_raise_returns_str() -> str:
    return "foo"

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = 2
    reveal_type(x)  # revealed: Literal[2]

reveal_type(x)  # revealed: str | Literal[2]
```

## Multiple `except` branches

If the scope reaches the final `reveal_type` call in this example, either the `try`-block suite of
statements was executed in its entirety, or exactly one `except` suite was executed in its entirety.
The inferred type of `x` at this point is the union of the types at the end of the three suites:

- At the end of `try`, `type(x) == str`
- At the end of `except TypeError`, `x == 2`
- At the end of `except ValueError`, `x == 3`

```py
def could_raise_returns_str() -> str:
    return "foo"

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = 2
    reveal_type(x)  # revealed: Literal[2]
except ValueError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = 3
    reveal_type(x)  # revealed: Literal[3]

reveal_type(x)  # revealed: str | Literal[2, 3]
```

## Exception handlers with `else` branches (but no `finally`)

If we reach the `reveal_type` call at the end of this scope, either the `try` and `else` suites were
both executed in their entireties, or the `except` suite was executed in its entirety. The type of
`x` at this point is the union of the type at the end of the `else` suite and the type at the end of
the `except` suite:

- At the end of `else`, `x == 3`
- At the end of `except`, `x == 2`

```py
def could_raise_returns_str() -> str:
    return "foo"

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = 2
    reveal_type(x)  # revealed: Literal[2]
else:
    reveal_type(x)  # revealed: str
    x = 3
    reveal_type(x)  # revealed: Literal[3]

reveal_type(x)  # revealed: Literal[2, 3]
```

For a block that has multiple `except` branches and an `else` branch, the same principle applies. In
order to reach the final `reveal_type` call, either exactly one of the `except` suites must have
been executed in its entirety, or the `try` suite and the `else` suite must both have been executed
in their entireties:

```py
x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = 2
    reveal_type(x)  # revealed: Literal[2]
except ValueError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = 3
    reveal_type(x)  # revealed: Literal[3]
else:
    reveal_type(x)  # revealed: str
    x = 4
    reveal_type(x)  # revealed: Literal[4]

reveal_type(x)  # revealed: Literal[2, 3, 4]
```

## Exception handlers with `finally` branches (but no `except` branches)

A `finally` suite is *always* executed. As such, if we reach the `reveal_type` call at the end of
this example, we know that `x` *must* have been reassigned to `2` during the `finally` suite. The
type of `x` at the end of the example is therefore `Literal[2]`:

```py
def could_raise_returns_str() -> str:
    return "foo"

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
finally:
    x = 2
    reveal_type(x)  # revealed: Literal[2]

reveal_type(x)  # revealed: Literal[2]
```

If `x` was *not* redefined in the `finally` suite, however, things are somewhat more complicated. If
we reach the final `reveal_type` call, unlike the state when we're visiting the `finally` suite, we
know that the `try`-block suite ran to completion. This means that there are fewer possible states
at this point than there were when we were inside the `finally` block.

(Our current model does *not* correctly infer the types *inside* `finally` suites, however; this is
still a TODO item for us.)

```py
x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
finally:
    # TODO: should be Literal[1] | str
    reveal_type(x)  # revealed: str

reveal_type(x)  # revealed: str
```

## Combining an `except` branch with a `finally` branch

As previously stated, we do not yet have accurate inference for types *inside* `finally` suites.
When we do, however, we will have to take account of the following possibilities inside `finally`
suites:

- The `try` suite could have run to completion
- Or we could have jumped from halfway through the `try` suite to an `except` suite, and the
    `except` suite ran to completion
- Or we could have jumped from halfway through the `try` suite straight to the `finally` suite due
    to an unhandled exception
- Or we could have jumped from halfway through the `try` suite to an `except` suite, only for an
    exception raised in the `except` suite to cause us to jump to the `finally` suite before the
    `except` suite ran to completion

```py
class A: ...
class B: ...
class C: ...

def could_raise_returns_A() -> A:
    return A()

def could_raise_returns_B() -> B:
    return B()

def could_raise_returns_C() -> C:
    return C()

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_A()
    reveal_type(x)  # revealed: A
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | A
    x = could_raise_returns_B()
    reveal_type(x)  # revealed: B
    x = could_raise_returns_C()
    reveal_type(x)  # revealed: C
finally:
    # TODO: should be `Literal[1] | A | B | C`
    reveal_type(x)  # revealed: A | C
    x = 2
    reveal_type(x)  # revealed: Literal[2]

reveal_type(x)  # revealed: Literal[2]
```

Now for an example without a redefinition in the `finally` suite. As before, there *should* be fewer
possibilities after completion of the `finally` suite than there were during the `finally` suite
itself. (In some control-flow possibilities, some exceptions were merely *suspended* during the
`finally` suite; these lead to the scope's termination following the conclusion of the `finally`
suite.)

```py
x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_A()
    reveal_type(x)  # revealed: A
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | A
    x = could_raise_returns_B()
    reveal_type(x)  # revealed: B
    x = could_raise_returns_C()
    reveal_type(x)  # revealed: C
finally:
    # TODO: should be `Literal[1] | A | B | C`
    reveal_type(x)  # revealed: A | C

reveal_type(x)  # revealed: A | C
```

An example with multiple `except` branches and a `finally` branch:

```py
class D: ...
class E: ...

def could_raise_returns_D() -> D:
    return D()

def could_raise_returns_E() -> E:
    return E()

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_A()
    reveal_type(x)  # revealed: A
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | A
    x = could_raise_returns_B()
    reveal_type(x)  # revealed: B
    x = could_raise_returns_C()
    reveal_type(x)  # revealed: C
except ValueError:
    reveal_type(x)  # revealed: Literal[1] | A
    x = could_raise_returns_D()
    reveal_type(x)  # revealed: D
    x = could_raise_returns_E()
    reveal_type(x)  # revealed: E
finally:
    # TODO: should be `Literal[1] | A | B | C | D | E`
    reveal_type(x)  # revealed: A | C | E

reveal_type(x)  # revealed: A | C | E
```

## Combining `except`, `else` and `finally` branches

If the exception handler has an `else` branch, we must also take into account the possibility that
control flow could have jumped to the `finally` suite from partway through the `else` suite due to
an exception raised *there*.

```py
class A: ...
class B: ...
class C: ...
class D: ...
class E: ...

def could_raise_returns_A() -> A:
    return A()

def could_raise_returns_B() -> B:
    return B()

def could_raise_returns_C() -> C:
    return C()

def could_raise_returns_D() -> D:
    return D()

def could_raise_returns_E() -> E:
    return E()

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_A()
    reveal_type(x)  # revealed: A
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | A
    x = could_raise_returns_B()
    reveal_type(x)  # revealed: B
    x = could_raise_returns_C()
    reveal_type(x)  # revealed: C
else:
    reveal_type(x)  # revealed: A
    x = could_raise_returns_D()
    reveal_type(x)  # revealed: D
    x = could_raise_returns_E()
    reveal_type(x)  # revealed: E
finally:
    # TODO: should be `Literal[1] | A | B | C | D | E`
    reveal_type(x)  # revealed: C | E

reveal_type(x)  # revealed: C | E
```

The same again, this time with multiple `except` branches:

```py
class F: ...
class G: ...

def could_raise_returns_F() -> F:
    return F()

def could_raise_returns_G() -> G:
    return G()

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_A()
    reveal_type(x)  # revealed: A
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | A
    x = could_raise_returns_B()
    reveal_type(x)  # revealed: B
    x = could_raise_returns_C()
    reveal_type(x)  # revealed: C
except ValueError:
    reveal_type(x)  # revealed: Literal[1] | A
    x = could_raise_returns_D()
    reveal_type(x)  # revealed: D
    x = could_raise_returns_E()
    reveal_type(x)  # revealed: E
else:
    reveal_type(x)  # revealed: A
    x = could_raise_returns_F()
    reveal_type(x)  # revealed: F
    x = could_raise_returns_G()
    reveal_type(x)  # revealed: G
finally:
    # TODO: should be `Literal[1] | A | B | C | D | E | F | G`
    reveal_type(x)  # revealed: C | E | G

reveal_type(x)  # revealed: C | E | G
```

## Nested `try`/`except` blocks

A checkpoint in a nested `try` suite propagates to both the nested and enclosing handlers unless the
nested statement has a bare handler. Checkpoints in its `except`, `else`, and `finally` suites
propagate only to the enclosing handler, because exceptions raised there are not handled by the same
`try` statement.

```py
class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
class F: ...
class G: ...
class H: ...
class I: ...
class J: ...
class K: ...

def could_raise_returns_A() -> A:
    return A()

def could_raise_returns_B() -> B:
    return B()

def could_raise_returns_C() -> C:
    return C()

def could_raise_returns_D() -> D:
    return D()

def could_raise_returns_E() -> E:
    return E()

def could_raise_returns_F() -> F:
    return F()

def could_raise_returns_G() -> G:
    return G()

def could_raise_returns_H() -> H:
    return H()

def could_raise_returns_I() -> I:
    return I()

def could_raise_returns_J() -> J:
    return J()

def could_raise_returns_K() -> K:
    return K()

x = 1

try:
    try:
        reveal_type(x)  # revealed: Literal[1]
        x = could_raise_returns_A()
        reveal_type(x)  # revealed: A
    except TypeError:
        reveal_type(x)  # revealed: Literal[1] | A
        x = could_raise_returns_B()
        reveal_type(x)  # revealed: B
        x = could_raise_returns_C()
        reveal_type(x)  # revealed: C
    except ValueError:
        reveal_type(x)  # revealed: Literal[1] | A
        x = could_raise_returns_D()
        reveal_type(x)  # revealed: D
        x = could_raise_returns_E()
        reveal_type(x)  # revealed: E
    else:
        reveal_type(x)  # revealed: A
        x = could_raise_returns_F()
        reveal_type(x)  # revealed: F
        x = could_raise_returns_G()
        reveal_type(x)  # revealed: G
    finally:
        # TODO: should be `Literal[1] | A | B | C | D | E | F | G`
        reveal_type(x)  # revealed: C | E | G
        x = 2
        reveal_type(x)  # revealed: Literal[2]
    reveal_type(x)  # revealed: Literal[2]
except:
    reveal_type(x)  # revealed: Literal[1, 2] | A | B | C | D | E | F | G
    x = could_raise_returns_H()
    reveal_type(x)  # revealed: H
    x = could_raise_returns_I()
    reveal_type(x)  # revealed: I
else:
    reveal_type(x)  # revealed: Literal[2]
    x = could_raise_returns_J()
    reveal_type(x)  # revealed: J
    x = could_raise_returns_K()
    reveal_type(x)  # revealed: K
finally:
    # TODO: should be `Literal[1, 2] | A | B | C | D | E | F | G | H | I | J | K`
    reveal_type(x)  # revealed: I | K

# Either one `except` branch or the `else`
# must have been taken and completed to get here:
reveal_type(x)  # revealed: I | K
```

## Nested scopes inside `try` blocks

Shadowing a variable in an inner scope has no effect on type inference of the variable by that name
in the outer scope:

```py
class A: ...
class B: ...
class C: ...
class D: ...
class E: ...

def could_raise_returns_A() -> A:
    return A()

def could_raise_returns_B() -> B:
    return B()

def could_raise_returns_C() -> C:
    return C()

def could_raise_returns_D() -> D:
    return D()

def could_raise_returns_E() -> E:
    return E()

x = 1

try:
    def foo(param=could_raise_returns_A()):
        x = could_raise_returns_A()

        try:
            reveal_type(x)  # revealed: A
            x = could_raise_returns_B()
            reveal_type(x)  # revealed: B
        except:
            reveal_type(x)  # revealed: A | B
            x = could_raise_returns_C()
            reveal_type(x)  # revealed: C
            x = could_raise_returns_D()
            reveal_type(x)  # revealed: D
        finally:
            # TODO: should be `A | B | C | D`
            reveal_type(x)  # revealed: B | D
        reveal_type(x)  # revealed: B | D
    x = foo
    reveal_type(x)  # revealed: def foo(param=...) -> Unknown
except:
    reveal_type(x)  # revealed: Literal[1] | (def foo(param=...) -> Unknown)

    class Bar:
        x = could_raise_returns_E()
        reveal_type(x)  # revealed: E

    x = Bar
    reveal_type(x)  # revealed: <class 'Bar'>
finally:
    # TODO: should be `Literal[1] | <class 'foo'> | <class 'Bar'>`
    reveal_type(x)  # revealed: (def foo(param=...) -> Unknown) | <class 'Bar'>

reveal_type(x)  # revealed: (def foo(param=...) -> Unknown) | <class 'Bar'>
```

[1]: https://astral-sh.notion.site/Exception-handler-control-flow-11348797e1ca80bb8ce1e9aedbbe439d

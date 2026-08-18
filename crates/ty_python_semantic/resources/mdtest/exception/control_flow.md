# Control flow for exception handlers

These tests describe which names are defined and what types they have in the branches of a
`try`/`except`/`else`/`finally` statement.

The analysis models exceptions from ordinary Python operations. It intentionally does not treat
every possible interruption, such as an exception raised by a signal handler, as an exception point.

For a full writeup on the semantics of exception handlers, see [this document][1].

Functions whose names start with `could_raise_` make it clear that a call may raise an exception
before an assignment completes. Any other function call can raise as well.

## Operations that cannot raise

Under this model, an exception handler is reachable only if the `try` block contains an operation
that can raise. Assigning a literal to a local name does not introduce an exception point:

```py
x = 1
try:
    x = 2
except:
    x = "unreachable"

reveal_type(x)  # revealed: Literal[2]
```

Testing literals, comparing identities, combining these conditions, and iterating over a list
literal cannot raise either:

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

## Annotated assignments that can raise

An annotation applies to assignments in the exception handler even if evaluating the annotated
assignment's right-hand side raises. In particular, it provides type context for a collection
literal in the handler.

```py
from typing import Any

def could_raise_dict() -> dict[str, Any]:
    return {}

def requires_str(value: str) -> None: ...
def fallback() -> None:
    try:
        result: dict[str, Any] = could_raise_dict()
    except Exception:
        result = {"correct": False, "message": "fallback"}
        reveal_type(result)  # revealed: dict[str, Any]

    reveal_type(result)  # revealed: dict[str, Any]
    requires_str(result["message"])
```

The declaration also rejects an incompatible assignment in the handler.

```py
def could_raise_int() -> int:
    return 1

def incompatible_fallback() -> None:
    try:
        value: int = could_raise_int()
    except Exception:
        value = "wrong"  # error: [invalid-assignment]
```

An earlier call in the `try` block does not hide a declaration reached before a later call raises.

```py
def declaration_after_call() -> None:
    value = int()
    try:
        could_raise_int()
        value: int = could_raise_int()
    except Exception:
        value = "wrong"  # error: [invalid-assignment]
```

The declaration does not make the new value available before the assignment completes. A handler
still sees the previous value, or an unbound name if there was no previous binding.

```py
def previous_binding() -> None:
    value = 0
    try:
        value: int = could_raise_int()
    except Exception:
        reveal_type(value)  # revealed: Literal[0]

def no_previous_binding() -> None:
    try:
        value: int = could_raise_int()
    except Exception:
        # error: [unresolved-reference]
        reveal_type(value)  # revealed: Unknown
```

A new annotation replaces an earlier declared type even if its right-hand side raises.

```py
def reannotated() -> None:
    value: object = None
    try:
        value: int = could_raise_int()
    except Exception:
        value = 1

    reveal_type(value)  # revealed: int
```

Assignments made while evaluating the right-hand side still reach the handler. When the call
returns, its result replaces the value assigned by the walrus expression on the successful path.

```py
from collections.abc import Callable
from typing import Literal

def assignment_in_rhs(could_raise_after: Callable[[int], Literal[3]]) -> None:
    value = 0
    try:
        value: int = could_raise_after(value := 2)
    except Exception:
        reveal_type(value)  # revealed: Literal[0, 2]
    else:
        reveal_type(value)  # revealed: Literal[3]

    reveal_type(value)  # revealed: Literal[0, 2, 3]
```

## Looking up an undefined name

An undefined name raises `NameError`, so an exception handler can provide its value:

```py
try:
    fallback  # ty: ignore[unresolved-reference]
except NameError:
    fallback = 1

def use_fallback() -> None:
    reveal_type(fallback)  # revealed: Literal[1]
```

A conditionally defined name may retain its original value or receive a value from the handler:

```py
def possibly_bound(flag: bool) -> None:
    if flag:
        value = 1

    try:
        value  # ty: ignore[possibly-unresolved-reference]
    except NameError:
        value = 2

    def use_value() -> None:
        reveal_type(value)  # revealed: Literal[1, 2]
```

A name that is definitely defined in the current scope cannot raise `NameError`:

```py
def definitely_bound(local_value: int) -> None:
    state = 0
    try:
        local_value
    except NameError:
        state = 1

    reveal_type(state)  # revealed: Literal[0]
```

A later local assignment can shadow a builtin and make an earlier reference raise
`UnboundLocalError`:

```py
def shadowed_builtin() -> None:
    try:
        int  # ty: ignore[unresolved-reference]
    except NameError:
        int = 1

    def use_shadowed_builtin() -> None:
        reveal_type(int)  # revealed: Literal[1]
```

## Undefined attribute and subscript receivers

An exception handler can also provide a missing name when that name is used as an attribute
receiver:

```py
try:
    receiver.attribute  # ty: ignore[unresolved-reference]
except NameError:
    receiver = object()

def use_receiver() -> None:
    reveal_type(receiver)  # revealed: object
```

Likewise, a subscript receiver may raise before its index is evaluated. The subscript itself may
raise after the index has been evaluated:

```py
state = "before"
try:
    state = 0
    missing[(state := 1)]  # ty: ignore[unresolved-reference]
except NameError:
    reveal_type(state)  # revealed: Literal[0, 1]
```

## Function arguments are evaluated before the call

If a function call raises, an assignment in one of its arguments has already completed:

```py
def may_raise(value: object) -> None: ...

x = 0
try:
    may_raise(x := 1)
except:
    reveal_type(x)  # revealed: Literal[1]
```

## Failed imports do not create bindings

When an import fails, its target has not been assigned. An exception handler can therefore provide a
fallback without conflicting with the imported module's type:

```py
try:
    import ssl
except ImportError:
    ssl = None
```

When importing several names, an earlier name may already be defined when a later import fails:

```py
first = 0
try:
    from collections.abc import Awaitable as first, Iterable as second
except ImportError:
    second = None
    reveal_type(first)  # revealed: Literal[0] | <class 'Awaitable'>
```

## Explicit raises and failing assertions

A `raise` statement runs after earlier assignments have completed:

```py
x = 1
try:
    x = 2
    raise RuntimeError
except:
    reveal_type(x)  # revealed: Literal[2]
```

A failing assertion preserves the narrowing implied by its failed condition:

```py
def check_assertion(x: int | None) -> None:
    try:
        assert x is not None
    except:
        reveal_type(x)  # revealed: None
```

Short-circuiting determines whether an assignment inside the assertion has run:

```py
def check_short_circuit_assertion(flag: bool) -> None:
    state = 2
    try:
        assert flag and (state := 0)
    except:
        reveal_type(state)  # revealed: Literal[2, 0]
```

## Attribute access and subscripting

Attribute access can raise after its receiver has been evaluated:

```py
class C:
    value: int

def attribute_access(c: C) -> None:
    state: C | int = 0
    try:
        (state := c).value
    except:
        reveal_type(state)  # revealed: C
```

A subscript can raise after its index has been evaluated:

```py
def subscript_access(values: list[int]) -> None:
    state = 0
    try:
        values[state := 1]
    except:
        reveal_type(state)  # revealed: Literal[1]
```

## Repeated potentially raising operations

Repeated calls with unchanged bindings do not alter the values visible to an exception handler, but
a later reassignment must still be included:

```py
def may_raise() -> None: ...
def repeated_calls() -> None:
    state = 0
    try:
        may_raise()
        may_raise()
        state = "changed"
        may_raise()
    except:
        reveal_type(state)  # revealed: Literal[0, "changed"]
```

A branch that does not change any bindings preserves the state visible to the handler:

```py
def unchanged_branch(flag: bool) -> None:
    state = 0
    try:
        may_raise()
        if flag is True:
            pass
        may_raise()
    except:
        reveal_type(state)  # revealed: Literal[0]
```

Branch narrowing changes the state visible to the handler even when neither branch introduces a new
binding:

```py
def narrowed_branch(value: int | None) -> None:
    try:
        if value is not None:
            may_raise()
            may_raise()
    except:
        reveal_type(value)  # revealed: int
```

Both sides of a restored branch remain visible when each can raise:

```py
def restored_branches(value: int | None) -> None:
    try:
        if value is not None:
            may_raise()
        else:
            may_raise()
    except:
        reveal_type(value)  # revealed: int | None
```

Match guards also distinguish successful and failed branches without introducing a new binding:

```py
def guarded_match_branches(value: int | None) -> None:
    try:
        match value:
            case _ if value is not None:
                may_raise()
            case _:
                may_raise()
    except:
        reveal_type(value)  # revealed: int | None
```

Deleting a binding changes the flow state even though the name remains present in the scope:

```py
def deleted_binding() -> None:
    state = 1
    try:
        may_raise()
        del state
        may_raise()
    except:
        # error: [possibly-unresolved-reference]
        reveal_type(state)  # revealed: Literal[1]
```

A call that cannot return still prevents later assignments from reaching the exception handler:

```py
from typing import NoReturn

def stop() -> NoReturn:
    raise RuntimeError

def call_never_returns() -> None:
    state = 0
    try:
        stop()
        state = "unreachable"
        may_raise()
    except:
        reveal_type(state)  # revealed: Literal[0]
```

## Nested handlers with merged bindings

An inner handler can preserve the original binding while its `else` suite sees a later assignment.
After those paths merge, an exception must expose both bindings to the outer handler:

```py
def may_raise() -> None: ...
def nested_try() -> None:
    state = 0
    try:
        try:
            may_raise()
            state = "changed"
        except:
            pass
        else:
            may_raise()
        may_raise()
    except:
        reveal_type(state)  # revealed: Literal[0, "changed"]
```

## Caught calls that never return

Catching an exception from a `NoReturn` call makes the following code reachable again, even if no
bindings changed. The unreachable inner `else` suite must not hide the later exception:

```py
from typing import NoReturn

def may_raise() -> None: ...
def stop() -> NoReturn:
    raise RuntimeError

def nested_terminal() -> None:
    state = 0
    try:
        try:
            stop()
        except:
            pass
        else:
            may_raise()
        may_raise()
    except:
        reveal_type(state)  # revealed: Literal[0]
```

## Operators and augmented assignments

An arithmetic operator can raise after evaluating both operands:

```py
class Number:
    def __truediv__(self, other: int) -> int:
        raise NotImplementedError

    def __lt__(self, other: int) -> bool:
        raise NotImplementedError

def division(number: Number) -> None:
    state = 0
    try:
        number / (state := 1)
    except:
        reveal_type(state)  # revealed: Literal[1]
```

A comparison is also evaluated after its operands:

```py
def comparison(number: Number) -> None:
    state = 0
    try:
        number < (state := 1)
    except:
        reveal_type(state)  # revealed: Literal[1]
```

Augmented assignment evaluates the target before its right-hand side. Reading the target can raise
before the right-hand side runs:

```py
def augmented_assignment(values: list[int]) -> None:
    target_state = 0
    rhs_state = 0
    try:
        values[target_state := 1] += (rhs_state := 1)
    except:
        reveal_type(target_state)  # revealed: Literal[1]
        reveal_type(rhs_state)  # revealed: Literal[0, 1]
```

## Conditions can raise

Evaluating an `if` condition can call `__bool__` or `__len__` and raise before its body runs:

```py
def if_condition(value: object) -> None:
    state = 0
    try:
        if value:
            state = 1
    except:
        reveal_type(state)  # revealed: Literal[0]
```

An assignment expression with a safe value cannot raise, including when it appears in an identity
comparison:

```py
def safe_named_expressions() -> None:
    caught = False
    try:
        if value := 1:
            pass
        if (value := 1) is not None:
            pass
    except:
        caught = True

    reveal_type(caught)  # revealed: Literal[False]
```

An assignment expression can still raise while calling its right-hand side or testing an unknown
value's truthiness:

```py
def unsafe_named_expressions(value: object, may_raise) -> None:
    caught = False
    try:
        if bound := may_raise():
            pass
    except:
        caught = True

    reveal_type(caught)  # revealed: bool

    caught = False
    try:
        if bound := value:
            pass
    except:
        caught = True

    reveal_type(caught)  # revealed: bool
```

A `while` condition can fail before its first iteration or after an earlier iteration:

```py
def while_condition(value: object) -> None:
    state = 0
    try:
        while value:
            state = 1
    except:
        reveal_type(state)  # revealed: Literal[0, 1]
```

## Pattern matching can raise

A sequence pattern can raise before its capture target or case body is assigned:

```py
def sequence_pattern(value: object) -> None:
    state = 0
    try:
        state = 1
        match value:
            case [item]:
                state = 2
    except:
        reveal_type(state)  # revealed: Literal[1]
        item  # error: [unresolved-reference]
```

Mapping, class, and literal patterns can call user-defined matching or equality operations:

```py
class Point:
    x: int

def mapping_pattern(value: object) -> None:
    state = 0
    try:
        state = 1
        match value:
            case {"x": item}:
                state = 2
    except:
        reveal_type(state)  # revealed: Literal[1]

def class_pattern(value: object) -> None:
    state = 0
    try:
        state = 1
        match value:
            case Point(x=item):
                state = 2
    except:
        reveal_type(state)  # revealed: Literal[1]

def literal_pattern(value: object) -> None:
    state = 0
    try:
        state = 1
        match value:
            case 1:
                state = 2
    except:
        reveal_type(state)  # revealed: Literal[1]
```

Wildcard, capture, and singleton patterns do not invoke user-defined operations:

```py
def safe_patterns(value: object) -> None:
    caught = False
    try:
        match value:
            case None:
                pass
            case captured:
                pass
        match value:
            case _:
                pass
    except:
        caught = True

    reveal_type(caught)  # revealed: Literal[False]
```

## Iteration can raise

An iterator can fail before producing its first item or after an earlier iteration has completed:

```py
from collections.abc import AsyncIterable, Iterable

def iteration(values: Iterable[int]) -> None:
    state = 0
    try:
        state = 1
        for _ in values:
            state = 2
    except:
        reveal_type(state)  # revealed: Literal[1, 2]
```

Assigning an iteration target can also fail before or after an earlier iteration:

```py
class C:
    value: int

def iteration_target(target: C) -> None:
    state = 0
    try:
        state = 1
        for target.value in [0, 1]:
            state = 2
    except:
        reveal_type(state)  # revealed: Literal[1, 2]
```

The same possibilities apply to asynchronous iteration:

```py
async def async_iteration(values: AsyncIterable[int]) -> None:
    state = 0
    try:
        state = 1
        async for _ in values:
            state = 2
    except:
        reveal_type(state)  # revealed: Literal[1, 2]
```

## Context-manager entry and exit can raise

A context manager can raise before its body runs or after the body completes:

```py
def context_manager_entry_and_exit(manager) -> None:
    state = 0
    try:
        with manager:
            state = 1
    except:
        reveal_type(state)  # revealed: Literal[0, 1]
```

Asynchronous context managers have the same entry and exit behavior:

```py
async def async_context_manager_entry_and_exit(manager) -> None:
    state = 0
    try:
        async with manager:
            state = 1
    except:
        reveal_type(state)  # revealed: Literal[0, 1]
```

If entering the context manager fails, its `as` target has not yet been assigned:

```py
def context_manager_target_may_be_unbound(manager) -> None:
    try:
        with manager as value:
            pass
    except:
        value  # error: [possibly-unresolved-reference]
```

Earlier context managers have already entered when a later manager raises:

```py
from typing import Literal

class FirstManager:
    def __enter__(self) -> Literal[1]:
        return 1

    def __exit__(self, *_):
        pass

def multiple_context_managers(first: FirstManager, second) -> None:
    state = 0
    try:
        with first as state, second:
            state = 2
    except:
        reveal_type(state)  # revealed: Literal[0, 1, 2]
```

## Unpacking can raise

Unpacking can fail before the assignments following it run:

```py
from collections.abc import Iterable

def unpacking(values: Iterable[int]) -> None:
    state = 0
    try:
        first, second = values
        state = 1
    except:
        reveal_type(state)  # revealed: Literal[0]
```

## Awaiting and yielding can raise

Awaiting can raise when a coroutine resumes:

```py
from collections.abc import Awaitable, Iterable

async def awaiting(value: Awaitable[int]) -> None:
    state = 0
    try:
        state = 1
        await value
    except:
        reveal_type(state)  # revealed: Literal[1]
```

Delegating to another iterable can raise while the generator is resumed:

```py
def yielding_from(values: Iterable[int]):
    state = 0
    try:
        state = 1
        yield from values
    except:
        reveal_type(state)  # revealed: Literal[1]
```

A plain `yield` can also raise when an exception is sent into the generator:

```py
def yielding():
    state = 0
    try:
        state = 1
        yield
    except:
        reveal_type(state)  # revealed: Literal[1]
```

## Immediately and lazily evaluated scopes

A class body runs immediately, so an exception raised there reaches the surrounding handler:

```py
def may_raise() -> None: ...

x = 0
try:
    class C:
        may_raise()

except:
    x = 1

reveal_type(x)  # revealed: Literal[0, 1]
```

A class-body assignment to a nonlocal variable is visible when the body raises:

```py
def class_nonlocal_assignment_raises() -> None:
    state = "before"
    try:
        class C:
            nonlocal state
            state = 1
            raise ValueError

    except ValueError:
        reveal_type(state)  # revealed: Literal["before", 1]
```

A nested class body also runs eagerly, so its nonlocal assignment reaches the surrounding handler:

```py
def nested_class_nonlocal_assignment_raises() -> None:
    state = "before"
    try:
        class Outer:
            class Inner:
                nonlocal state
                state = 1
                raise ValueError

    except ValueError:
        reveal_type(state)  # revealed: Literal["before", 1]
```

A class can fail during construction even when its body contains only an assignment:

```py
def class_construction_can_raise() -> None:
    state = "before"
    caught = False
    try:
        class C:
            nonlocal state
            state = 1

    except:
        caught = True

    reveal_type(caught)  # revealed: bool
```

A class-construction hook runs after the class body's nonlocal assignment:

```py
class RaisingBase:
    def __init_subclass__(cls) -> None:
        raise ValueError

def class_construction_hook_raises() -> None:
    state = 0
    try:
        class C(RaisingBase):
            nonlocal state
            state = 1

    except ValueError:
        reveal_type(state)  # revealed: Literal[0, 1]
```

A class decorator is applied after the class body has run:

```py
def class_decorator_raises(decorator) -> None:
    state = 0
    try:
        @decorator
        class C:
            nonlocal state
            state = 1

    except ValueError:
        reveal_type(state)  # revealed: Literal[0, 1]
```

A function decorator is applied after its parameter defaults have been evaluated:

```py
def function_decorator_raises(decorator) -> None:
    state = 0
    try:
        @decorator
        def inner(value=(state := 1)) -> None:
            pass

    except Exception:
        reveal_type(state)  # revealed: Literal[1]
```

Decorator application can also raise when the function has no parameter defaults:

```py
def function_decorator_without_defaults(decorator) -> None:
    caught = False
    try:
        @decorator
        def inner() -> None:
            pass

    except Exception:
        caught = True

    reveal_type(caught)  # revealed: bool
```

A list comprehension also runs immediately:

```py
y = 0
try:
    [may_raise() for _ in [0]]
except:
    y = 1

reveal_type(y)  # revealed: Literal[0, 1]
```

Generator expressions are also assumed to run eagerly for exception-flow analysis, since in practice
they are almost always eagerly consumed in real-world code:

```py
z = 0
try:
    (may_raise() for _ in [0])
except:
    z = 1

reveal_type(z)  # revealed: Literal[0, 1]
```

A nested function body also runs later, so its exceptions cannot reach the handler surrounding its
definition:

```py
function_caught = False
try:
    def nested_function() -> None:
        may_raise()

except:
    function_caught = True

reveal_type(function_caught)  # revealed: Literal[False]
```

An exception handler inside a lazily evaluated function still catches exceptions raised within that
function:

```py
outer_caught = False
try:
    def nested_function_with_handler() -> None:
        inner_caught = False
        try:
            may_raise()
        except:
            inner_caught = True

        reveal_type(inner_caught)  # revealed: bool

except:
    outer_caught = True

reveal_type(outer_caught)  # revealed: Literal[False]
```

## Assignments in comprehensions

A handler includes the value from before a comprehension and the value visible once it finishes.
Assignments overwritten inside the comprehension are not tracked separately, while normal completion
still preserves the final assignment:

```py
def comprehension_may_raise() -> None: ...
def overwritten_comprehension_assignment() -> None:
    state = None
    try:
        [(state := 1, comprehension_may_raise(), state := "later") for _ in [0]]
    except:
        # TODO: Include `int` from the assignment before the raising call.
        reveal_type(state)  # revealed: None | str
        return

    reveal_type(state)  # revealed: str
```

Dictionary comprehensions are also evaluated eagerly:

```py
def dict_comprehension_assignment() -> None:
    state = "before"
    try:
        {item: (state := 1, comprehension_may_raise()) for item in [0]}
    except:
        reveal_type(state)  # revealed: Literal["before"] | int
```

## Assignments in generator expressions

Generator expressions are assumed to run eagerly, so their assignments and calls can reach the
surrounding exception handler. Strictly speaking generator expressions *can* be lazy, but in
practice they are almost always eagerly consumed in real-world code:

```py
def generator_may_raise() -> None: ...
def generator_assignment() -> None:
    state = 0
    caught = False
    try:
        ((state := 1, generator_may_raise()) for _ in [0])
    except:
        reveal_type(state)  # revealed: int
        caught = True

    reveal_type(caught)  # revealed: bool
    reveal_type(state)  # revealed: int
```

## Nested comprehension assignments

An assignment in an inner comprehension still updates the scope containing the outermost
comprehension:

```py
def comprehension_may_raise() -> None: ...
def nested_comprehension_assignments() -> None:
    state = None
    try:
        [[(state := 1, comprehension_may_raise()) for _ in [0]] for _ in [0]]
    except:
        reveal_type(state)  # revealed: None | int
```

## Module and global assignments in comprehensions

An assignment expression in a module-level comprehension updates the module-level name:

```py
def comprehension_may_raise() -> None: ...

module_comprehension_state = "before"
try:
    [(module_comprehension_state := 1, comprehension_may_raise()) for _ in [0]]
except:
    reveal_type(module_comprehension_state)  # revealed: Literal["before"] | int
```

An explicitly global assignment updates the same name from inside a function:

```py
global_comprehension_state = "before"

def global_comprehension_assignment() -> None:
    global global_comprehension_state
    try:
        [(global_comprehension_state := 1, comprehension_may_raise()) for _ in [0]]
    except:
        reveal_type(global_comprehension_state)  # revealed: int | Literal["before"]
```

## Assignments in asynchronous comprehensions

An asynchronous comprehension can raise during iteration or after an assignment has completed:

```py
from collections.abc import AsyncIterable, Awaitable

async def async_comprehension_assignment(values: AsyncIterable[int], awaitable: Awaitable[int]) -> None:
    state = "before"
    try:
        state = "ready"
        [(state := 1, await awaitable) async for _ in values]
    except:
        reveal_type(state)  # revealed: Literal["ready"] | int
```

## Exceptions passing through a `finally` clause

An outer handler includes assignments from an intervening `finally` clause:

```py
state = 0
try:
    try:
        state = 1
        raise ValueError
    finally:
        state = 2
except ValueError:
    reveal_type(state)  # revealed: Literal[1, 2]
```

Cleanup also runs before an exception escapes an inner handler for a different exception type:

```py
state = 0
try:
    try:
        state = 1
        raise ValueError
    except TypeError:
        state = 3
    finally:
        state = 2
except ValueError:
    reveal_type(state)  # revealed: Literal[1, 2]
```

An exception path must not contaminate the normal continuation after cleanup:

```py
def may_raise() -> None: ...

state = 0
try:
    try:
        may_raise()
        state = 1
    finally:
        pass

    reveal_type(state)  # revealed: Literal[1]
except:
    pass
```

Cleanup remains visible when earlier and later calls share the same exception checkpoint:

```py
state = 0
try:
    may_raise()
    try:
        may_raise()
        state = 1
    finally:
        state = 2
except:
    reveal_type(state)  # revealed: Literal[0, 2]
```

A return passing through non-raising cleanup does not make an outer exception handler reachable:

```py
def return_through_cleanup() -> None:
    try:
        try:
            return
        finally:
            state = 2
    except:
        reveal_type(state)  # revealed: Never
```

## Nested exception handlers

A bare inner handler catches an exception before it can reach the outer handler:

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
```

An exception raised inside the inner handler can still reach the outer handler:

```py
try:
    try:
        may_raise()
    except:
        x = 3
        may_raise()
except:
    reveal_type(x)  # revealed: Literal[3]
```

A newly entered inner handler receives exceptions even if an earlier call already reached the outer
handler without changing any bindings:

```py
def inner_handler_after_outer_checkpoint() -> None:
    try:
        may_raise()
        try:
            may_raise()
        except:
            caught_inside = True

            reveal_type(caught_inside)  # revealed: Literal[True]
    except:
        pass
```

Code in an unreachable inner handler cannot make the outer handler reachable:

```py
z = 0
try:
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

# Control flow for exception handlers

These tests assert that we understand the possible "definition states" (which symbols might or might
not be defined) in the various branches of a `try`/`except`/`else`/`finally` block.

For a full writeup on the semantics of exception handlers, see [this document][1].

The tests throughout this Markdown document use functions with names starting with `could_raise_*`
to mark definitions that might or might not succeed (as the function could raise an exception). A
type checker must assume that any arbitrary function call could raise an exception in Python; this
is just a naming convention used in these tests for clarity, and to future-proof the tests against
possible future improvements whereby certain statements or expressions could potentially be inferred
as being incapable of causing an exception to be raised.

## A single bare `except`

Consider the following `try`/`except` block, with a single bare `except:`. There are different types
for the variable `x` in the two branches of this block, and we can't determine which branch might
have been taken from the perspective of code following this block. The inferred type after the
block's conclusion is therefore the union of the type at the end of the `try` suite (`str`) and the
type at the end of the `except` suite (`Literal[2]`).

*Within* the `except` suite, we must infer a union of all possible "definition states" we could have
been in at any point during the `try` suite. This is because control flow could have jumped to the
`except` suite without any of the `try`-suite definitions successfully completing, with only *some*
of the `try`-suite definitions successfully completing, or indeed with *all* of them successfully
completing. The type of `x` at the beginning of the `except` suite in this example is therefore
`Literal[1] | str`, taking into account that we might have jumped to the `except` suite before the
`x = could_raise_returns_str()` redefinition, but we *also* could have jumped to the `except` suite
*after* that redefinition.

```py path=union_type_inferred.py
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

```py path=branches_unify_to_non_union_type.py
def could_raise_returns_str() -> str:
    return "foo"

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

```py path=single_except.py
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

```py path=redef_in_finally.py
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

```py path=no_redef_in_finally.py
def could_raise_returns_str() -> str:
    return "foo"

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

```py path=redef_in_finally.py
def could_raise_returns_str() -> str:
    return "foo"

def could_raise_returns_bytes() -> bytes:
    return b"foo"

def could_raise_returns_bool() -> bool:
    return True

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = could_raise_returns_bytes()
    reveal_type(x)  # revealed: bytes
    x = could_raise_returns_bool()
    reveal_type(x)  # revealed: bool
finally:
    # TODO: should be `Literal[1] | str | bytes | bool`
    reveal_type(x)  # revealed: str | bool
    x = 2
    reveal_type(x)  # revealed: Literal[2]

reveal_type(x)  # revealed: Literal[2]
```

Now for an example without a redefinition in the `finally` suite. As before, there *should* be fewer
possibilities after completion of the `finally` suite than there were during the `finally` suite
itself. (In some control-flow possibilities, some exceptions were merely *suspended* during the
`finally` suite; these lead to the scope's termination following the conclusion of the `finally`
suite.)

```py path=no_redef_in_finally.py
def could_raise_returns_str() -> str:
    return "foo"

def could_raise_returns_bytes() -> bytes:
    return b"foo"

def could_raise_returns_bool() -> bool:
    return True

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = could_raise_returns_bytes()
    reveal_type(x)  # revealed: bytes
    x = could_raise_returns_bool()
    reveal_type(x)  # revealed: bool
finally:
    # TODO: should be `Literal[1] | str | bytes | bool`
    reveal_type(x)  # revealed: str | bool

reveal_type(x)  # revealed: str | bool
```

An example with multiple `except` branches and a `finally` branch:

```py path=multiple_except_branches.py
def could_raise_returns_str() -> str:
    return "foo"

def could_raise_returns_bytes() -> bytes:
    return b"foo"

def could_raise_returns_bool() -> bool:
    return True

def could_raise_returns_memoryview() -> memoryview:
    return memoryview(b"")

def could_raise_returns_float() -> float:
    return 3.14

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = could_raise_returns_bytes()
    reveal_type(x)  # revealed: bytes
    x = could_raise_returns_bool()
    reveal_type(x)  # revealed: bool
except ValueError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = could_raise_returns_memoryview()
    reveal_type(x)  # revealed: memoryview
    x = could_raise_returns_float()
    reveal_type(x)  # revealed: float
finally:
    # TODO: should be `Literal[1] | str | bytes | bool | memoryview | float`
    reveal_type(x)  # revealed: str | bool | float

reveal_type(x)  # revealed: str | bool | float
```

## Combining `except`, `else` and `finally` branches

If the exception handler has an `else` branch, we must also take into account the possibility that
control flow could have jumped to the `finally` suite from partway through the `else` suite due to
an exception raised *there*.

```py path=single_except_branch.py
def could_raise_returns_str() -> str:
    return "foo"

def could_raise_returns_bytes() -> bytes:
    return b"foo"

def could_raise_returns_bool() -> bool:
    return True

def could_raise_returns_memoryview() -> memoryview:
    return memoryview(b"")

def could_raise_returns_float() -> float:
    return 3.14

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = could_raise_returns_bytes()
    reveal_type(x)  # revealed: bytes
    x = could_raise_returns_bool()
    reveal_type(x)  # revealed: bool
else:
    reveal_type(x)  # revealed: str
    x = could_raise_returns_memoryview()
    reveal_type(x)  # revealed: memoryview
    x = could_raise_returns_float()
    reveal_type(x)  # revealed: float
finally:
    # TODO: should be `Literal[1] | str | bytes | bool | memoryview | float`
    reveal_type(x)  # revealed: bool | float

reveal_type(x)  # revealed: bool | float
```

The same again, this time with multiple `except` branches:

```py path=multiple_except_branches.py
def could_raise_returns_str() -> str:
    return "foo"

def could_raise_returns_bytes() -> bytes:
    return b"foo"

def could_raise_returns_bool() -> bool:
    return True

def could_raise_returns_memoryview() -> memoryview:
    return memoryview(b"")

def could_raise_returns_float() -> float:
    return 3.14

def could_raise_returns_range() -> range:
    return range(42)

def could_raise_returns_slice() -> slice:
    return slice(None)

x = 1

try:
    reveal_type(x)  # revealed: Literal[1]
    x = could_raise_returns_str()
    reveal_type(x)  # revealed: str
except TypeError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = could_raise_returns_bytes()
    reveal_type(x)  # revealed: bytes
    x = could_raise_returns_bool()
    reveal_type(x)  # revealed: bool
except ValueError:
    reveal_type(x)  # revealed: Literal[1] | str
    x = could_raise_returns_memoryview()
    reveal_type(x)  # revealed: memoryview
    x = could_raise_returns_float()
    reveal_type(x)  # revealed: float
else:
    reveal_type(x)  # revealed: str
    x = could_raise_returns_range()
    reveal_type(x)  # revealed: range
    x = could_raise_returns_slice()
    reveal_type(x)  # revealed: slice
finally:
    # TODO: should be `Literal[1] | str | bytes | bool | memoryview | float | range | slice`
    reveal_type(x)  # revealed: bool | float | slice

reveal_type(x)  # revealed: bool | float | slice
```

## Nested `try`/`except` blocks

It would take advanced analysis, which we are not yet capable of, to be able to determine that an
exception handler always suppresses all exceptions. This is partly because it is possible for
statements in `except`, `else` and `finally` suites to raise exceptions as well as statements in
`try` suites. This means that if an exception handler is nested inside the `try` statement of an
enclosing exception handler, it should (at least for now) be treated the same as any other node: as
a suite containing statements that could possibly raise exceptions, which would lead to control flow
jumping out of that suite prior to the suite running to completion.

```py
def could_raise_returns_str() -> str:
    return "foo"

def could_raise_returns_bytes() -> bytes:
    return b"foo"

def could_raise_returns_bool() -> bool:
    return True

def could_raise_returns_memoryview() -> memoryview:
    return memoryview(b"")

def could_raise_returns_float() -> float:
    return 3.14

def could_raise_returns_range() -> range:
    return range(42)

def could_raise_returns_slice() -> slice:
    return slice(None)

def could_raise_returns_complex() -> complex:
    return 3j

def could_raise_returns_bytearray() -> bytearray:
    return bytearray()

class Foo: ...
class Bar: ...

def could_raise_returns_Foo() -> Foo:
    return Foo()

def could_raise_returns_Bar() -> Bar:
    return Bar()

x = 1

try:
    try:
        reveal_type(x)  # revealed: Literal[1]
        x = could_raise_returns_str()
        reveal_type(x)  # revealed: str
    except TypeError:
        reveal_type(x)  # revealed: Literal[1] | str
        x = could_raise_returns_bytes()
        reveal_type(x)  # revealed: bytes
        x = could_raise_returns_bool()
        reveal_type(x)  # revealed: bool
    except ValueError:
        reveal_type(x)  # revealed: Literal[1] | str
        x = could_raise_returns_memoryview()
        reveal_type(x)  # revealed: memoryview
        x = could_raise_returns_float()
        reveal_type(x)  # revealed: float
    else:
        reveal_type(x)  # revealed: str
        x = could_raise_returns_range()
        reveal_type(x)  # revealed: range
        x = could_raise_returns_slice()
        reveal_type(x)  # revealed: slice
    finally:
        # TODO: should be `Literal[1] | str | bytes | bool | memoryview | float | range | slice`
        reveal_type(x)  # revealed: bool | float | slice
        x = 2
        reveal_type(x)  # revealed: Literal[2]
    reveal_type(x)  # revealed: Literal[2]
except:
    reveal_type(x)  # revealed: Literal[1, 2] | str | bytes | bool | memoryview | float | range | slice
    x = could_raise_returns_complex()
    reveal_type(x)  # revealed: complex
    x = could_raise_returns_bytearray()
    reveal_type(x)  # revealed: bytearray
else:
    reveal_type(x)  # revealed: Literal[2]
    x = could_raise_returns_Foo()
    reveal_type(x)  # revealed: Foo
    x = could_raise_returns_Bar()
    reveal_type(x)  # revealed: Bar
finally:
    # TODO: should be `Literal[1, 2] | str | bytes | bool | memoryview | float | range | slice | complex | bytearray | Foo | Bar`
    reveal_type(x)  # revealed: bytearray | Bar

# Either one `except` branch or the `else`
# must have been taken and completed to get here:
reveal_type(x)  # revealed: bytearray | Bar
```

## Nested scopes inside `try` blocks

Shadowing a variable in an inner scope has no effect on type inference of the variable by that name
in the outer scope:

```py
def could_raise_returns_str() -> str:
    return "foo"

def could_raise_returns_bytes() -> bytes:
    return b"foo"

def could_raise_returns_range() -> range:
    return range(42)

def could_raise_returns_bytearray() -> bytearray:
    return bytearray()

def could_raise_returns_float() -> float:
    return 3.14

x = 1

try:

    def foo(param=could_raise_returns_str()):
        x = could_raise_returns_str()

        try:
            reveal_type(x)  # revealed: str
            x = could_raise_returns_bytes()
            reveal_type(x)  # revealed: bytes
        except:
            reveal_type(x)  # revealed: str | bytes
            x = could_raise_returns_bytearray()
            reveal_type(x)  # revealed: bytearray
            x = could_raise_returns_float()
            reveal_type(x)  # revealed: float
        finally:
            # TODO: should be `str | bytes | bytearray | float`
            reveal_type(x)  # revealed: bytes | float
        reveal_type(x)  # revealed: bytes | float
    x = foo
    reveal_type(x)  # revealed: Literal[foo]
except:
    reveal_type(x)  # revealed: Literal[1] | Literal[foo]

    class Bar:
        x = could_raise_returns_range()
        reveal_type(x)  # revealed: range

    x = Bar
    reveal_type(x)  # revealed: Literal[Bar]
finally:
    # TODO: should be `Literal[1] | Literal[foo] | Literal[Bar]`
    reveal_type(x)  # revealed: Literal[foo] | Literal[Bar]

reveal_type(x)  # revealed: Literal[foo] | Literal[Bar]
```

[1]: https://astral-sh.notion.site/Exception-handler-control-flow-11348797e1ca80bb8ce1e9aedbbe439d

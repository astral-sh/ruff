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

It would take advanced analysis, which we are not yet capable of, to be able to determine that an
exception handler always suppresses all exceptions. This is partly because it is possible for
statements in `except`, `else` and `finally` suites to raise exceptions as well as statements in
`try` suites. This means that if an exception handler is nested inside the `try` statement of an
enclosing exception handler, it should (at least for now) be treated the same as any other node: as
a suite containing statements that could possibly raise exceptions, which would lead to control flow
jumping out of that suite prior to the suite running to completion.

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
    reveal_type(x)  # revealed: Literal[foo]
except:
    reveal_type(x)  # revealed: Literal[1] | Literal[foo]

    class Bar:
        x = could_raise_returns_E()
        reveal_type(x)  # revealed: E

    x = Bar
    reveal_type(x)  # revealed: Literal[Bar]
finally:
    # TODO: should be `Literal[1] | Literal[foo] | Literal[Bar]`
    reveal_type(x)  # revealed: Literal[foo] | Literal[Bar]

reveal_type(x)  # revealed: Literal[foo] | Literal[Bar]
```

[1]: https://astral-sh.notion.site/Exception-handler-control-flow-11348797e1ca80bb8ce1e9aedbbe439d

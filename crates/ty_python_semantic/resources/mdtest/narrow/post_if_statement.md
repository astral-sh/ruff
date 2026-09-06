# Consolidating narrowed types after if statement

## After if-else statements, narrowing has no effect if the variable is not mutated in any branch

```py
def _(x: int | None):
    if x is None:
        pass
    else:
        pass

    reveal_type(x)  # revealed: int | None
```

Calls in both branches must not prevent complementary narrowing paths from recombining into the
original type.

```py
class Base: ...
class Child(Base): ...

def consume(value: object) -> None: ...
def _(value: Base):
    if isinstance(value, Child):
        consume(value)
    else:
        consume(value)

    reveal_type(value)  # revealed: Base
```

## Narrowing can have a persistent effect if the variable is mutated in one branch

```py
def _(x: int | None):
    if x is None:
        x = 10
    else:
        pass

    reveal_type(x)  # revealed: int
```

## An if statement without an explicit `else` branch is equivalent to one with a no-op `else` branch

```py
def _(x: int | None, y: int | None):
    if x is None:
        x = 0

    if y is None:
        pass

    reveal_type(x)  # revealed: int
    reveal_type(y)  # revealed: int | None
```

## An if-elif without an explicit else branch is equivalent to one with an empty else branch

```py
def _(x: int | None):
    if x is None:
        x = 0
    elif x > 50:
        x = 50

    reveal_type(x)  # revealed: int
```

## Narrowing is preserved when a terminal branch prevents a path from flowing through

When one branch of an if/elif/else is terminal (e.g. contains `return`), narrowing from the
non-terminal branches is preserved after the merge point.

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A):
        pass
    elif isinstance(x, B):
        pass
    else:
        return

    # Only the if-branch (A) and elif-branch (B) flow through.
    # The else-branch returned, so its narrowing doesn't participate.
    reveal_type(x)  # revealed: B | A
```

## Narrowing is preserved with multiple terminal branches

```py
class A: ...
class B: ...
class C: ...
class D: ...

def _(x: A | B | C | D):
    if isinstance(x, A):
        return
    elif isinstance(x, B):
        pass
    elif isinstance(x, C):
        pass
    else:
        return

    # Only the elif-B and elif-C branches flow through.
    reveal_type(x)  # revealed: (C & ~A & ~B) | (B & ~A)
```

## Opaque branch predicates should not manufacture narrowing

If a boolean flag is set by an opaque branch predicate, checking that flag later should not make an
unknown value look more precise than it really is just because other branches contained narrowing
checks.

```py
def cond(x) -> bool:
    return bool(x)

def _(x):
    flag = False
    if cond(x):
        flag = True
    elif isinstance(x, float):
        return
    elif isinstance(x, int):
        return

    if flag:
        reveal_type(x)  # revealed: Unknown
```

## Multiple sequential if-statements don't leak narrowing

After a complete if/else where both branches flow through (no terminal), narrowing should be
cancelled out at the merge point.

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A):
        pass
    else:
        pass

    # Narrowing cancels out: both paths flow, so type is unchanged.
    reveal_type(x)  # revealed: A | B | C

    if isinstance(x, B):
        pass
    else:
        pass

    # Second if-statement's narrowing also cancels out.
    reveal_type(x)  # revealed: A | B | C
```

## Narrowing after a `NoReturn` call in one branch

When a branch calls a function that returns `NoReturn`/`Never`, we know that branch terminates and
doesn't contribute to the type after the if statement.

```py
import sys

def _(val: int | None):
    if val is None:
        sys.exit()
    reveal_type(val)  # revealed: int
```

Narrowing from the terminal branch is also preserved when deciding whether a later overloaded call
returns:

```py
from typing import Literal, overload
from typing_extensions import Never

def abort() -> Never:
    raise RuntimeError

@overload
def terminal(value: Literal[0]) -> Never: ...
@overload
def terminal(value: Literal[1]) -> None: ...
def terminal(value: Literal[0, 1]) -> None:
    if value == 0:
        raise RuntimeError

def _(value: Literal[0, 1]) -> None:
    if value == 1:
        abort()
    terminal(value)
    return "unreachable"
```

This also works when the `NoReturn` function is called in the else branch:

```py
import sys

def _(val: int | None):
    if val is not None:
        pass
    else:
        sys.exit()
    reveal_type(val)  # revealed: int
```

Narrowing that occurs after the `NoReturn` call must also be discarded with the unreachable branch:

```py
from typing_extensions import Never

def fail() -> Never:
    raise RuntimeError

def _(x: int | None, flag: bool):
    if flag:
        fail()
        if x is not None:
            return
    else:
        if x is None:
            return

    reveal_type(x)  # revealed: int
```

Call constraints that precede a nested merge must still gate narrowing later in the outer branch:

```py
from typing_extensions import Never

def fail_nested_merge() -> Never:
    raise RuntimeError

def _(x: int | None, outer: bool, inner: bool) -> None:
    if outer:
        fail_nested_merge()

        if inner:
            pass
        else:
            pass

        if x is not None:
            return
    else:
        if x is None:
            return

    reveal_type(x)  # revealed: int
```

Call constraints introduced inside the nested branches are still discarded at that merge:

```py
def _(x: int | None, outer: bool, inner: bool) -> None:
    if outer:
        if inner:
            pass
        else:
            fail_nested_merge()

        if x is not None:
            return
    else:
        if x is None:
            return

    reveal_type(x)  # revealed: None | int
```

If every nested branch contains a call, their combined call constraint must be preserved:

```py
def _(x: int | None, outer: bool, inner: bool) -> None:
    if outer:
        if inner:
            fail_nested_merge()
        else:
            fail_nested_merge()

        if x is not None:
            return
    else:
        if x is None:
            return

    reveal_type(x)  # revealed: int
```

And for elif branches:

```py
import sys

def _(val: int | str | None):
    if val is None:
        sys.exit()
    elif isinstance(val, int):
        pass
    else:
        sys.exit()
    reveal_type(val)  # revealed: int
```

## Narrowing through statically known branches

When a terminal (`return`) is inside the reachable branch of a statically known condition, narrowing
propagates through because the unreachable branch contributes `Never` to the union.

```py
def _(x: int | None):
    if True:
        if x is None:
            return
        reveal_type(x)  # revealed: int
    reveal_type(x)  # revealed: int
```

```py
from typing import Final

def _(x: int | None):
    if 1 + 1 == 2:
        if x is None:
            return
        reveal_type(x)  # revealed: int

    reveal_type(x)  # revealed: int

def _(x: int | None):
    if 1 + 1 != 2:
        pass
    else:
        if x is None:
            return
        reveal_type(x)  # revealed: int

    reveal_type(x)  # revealed: int

def _(x: int | None, flag: bool):
    if 1 + 1 == 2 or flag:
        if x is None:
            return

    reveal_type(x)  # revealed: int

def _(x: int | None, flag: bool):
    if 1 + 1 != 2 and flag:
        pass
    else:
        if x is None:
            return

    reveal_type(x)  # revealed: int

def _(x: int | None, flag: bool):
    if flag:
        if x is None:
            return

    # An ambiguous condition must not make its other branch unreachable.
    reveal_type(x)  # revealed: int | None

needs_inference: Final = True

def _(x: int | None):
    if needs_inference:
        if x is None:
            return
        reveal_type(x)  # revealed: int

    reveal_type(x)  # revealed: int
```

This also works when the always-true condition is nested inside a narrowing branch:

```py
from typing import Literal

def _(x: int | None):
    if x is None:
        if 1 + 1 == 2:
            return

    reveal_type(x)  # revealed: int

def _(x: int | None):
    if x is None:
        if needs_inference:
            return

    reveal_type(x)  # revealed: int

def always_true(value: object) -> Literal[True]:
    return True

def _(x: int | None):
    if x is None:
        if always_true(x):
            return

    reveal_type(x)  # revealed: int
```

## Statically known branches inside module and class loops

Narrowing also propagates through a statically known branch inside a module-level loop.

```py
def get_value() -> int | None:
    return None

while bool(input()):
    value = get_value()
    if 1 + 1 == 2:
        if value is None:
            raise RuntimeError

    reveal_type(value)  # revealed: int
```

The same condition narrows a value inside a class-body loop.

```py
class Example:
    while bool(input()):
        value = get_value()
        if 1 + 1 == 2:
            if value is None:
                raise RuntimeError

        reveal_type(value)  # revealed: int
```

## Narrowing from `assert` should not affect reassigned variables

When a variable is reassigned after an `assert`, the narrowing from the assert should not apply to
the new value.

```py
def foo(arg: int) -> int | None:
    return None

def bar() -> None:
    v = foo(1)
    assert v is None

    v = foo(2)
    # v was reassigned, so the assert narrowing shouldn't apply
    reveal_type(v)  # revealed: int | None
```

## Narrowing from `NoReturn` should not affect reassigned variables

When a variable is narrowed due to a `NoReturn` call in one branch and then reassigned, the
narrowing should only apply before the reassignment, not after.

```py
import sys

def foo() -> int | None:
    return 3

def bar():
    v = foo()
    if v is None:
        sys.exit()
    reveal_type(v)  # revealed: int

    v = foo()
    # v was reassigned, so any narrowing shouldn't apply
    reveal_type(v)  # revealed: int | None
```

## Narrowing preserved when `await`ing a `NoReturn` function in one branch

```py
from typing import NoReturn

async def stop() -> NoReturn:
    raise NotImplementedError

async def main(val: int | None):
    if val is None:
        await stop()
    reveal_type(val)  # revealed: int
```

## Narrowing in global scope

```py
data: dict[str, str] = {}
api_key = data.get("api_key")

if not api_key:
    exit(1)

reveal_type(api_key)  # revealed: str & ~AlwaysFalsy
```

# Narrowing with aliased conditions

Narrowing is supported when a narrowing expression is stored in an intermediate variable (an
"aliased conditional expression") and that variable is later used as a condition.

## `is None` alias

```py
def _(x: int | None):
    is_none = x is None
    if is_none:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: int

def _(x: int | None):
    is_none: bool = x is None
    if is_none:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: int
```

## `is not None` alias

```py
def _(x: int | None):
    is_not_none = x is not None
    if is_not_none:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: None
```

## `isinstance` alias

```py
def _(x: int | None):
    is_int = isinstance(x, int)
    if is_int:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: None
```

## Equality comparisons

```py
from typing import Literal

def _(x: Literal[1, 2]):
    is_one = x == 1
    if is_one:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: Literal[2]
```

## `TypeGuard`/`TypeIs` alias

```py
from typing_extensions import TypeGuard, TypeIs

def is_int(x: object) -> TypeGuard[int]:
    return isinstance(x, int)

def _(x: int | None):
    is_i = is_int(x)
    if is_i:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: int | None

def is_int2(x: object) -> TypeIs[int]:
    return isinstance(x, int)

def _(x: int | None):
    is_i = is_int2(x)
    if is_i:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: None
```

## `if` expression alias

```py
def _(x: int | None):
    is_none = x is None if True else False
    if is_none:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: int
```

## `bool()` alias

```py
def _(x: int | None):
    is_none = bool(x is None)
    if is_none:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: int
```

## Negated alias with `not`

```py
def _(x: int | None):
    is_none = x is None
    if not is_none:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: None
```

## Boolean-operated alias

```py
def _(x: str | int | None):
    is_none = x is None
    is_int = isinstance(x, int)
    if is_none:
        reveal_type(x)  # revealed: None
    if is_int:
        reveal_type(x)  # revealed: int
    if is_none or is_int:
        reveal_type(is_none)  # revealed: bool
        reveal_type(x)  # revealed: None | int
    if is_none and is_int:
        reveal_type(is_none)  # revealed: Literal[True]
        reveal_type(x)  # revealed: Never
    if not (is_none or is_int):
        reveal_type(is_none)  # revealed: Literal[False]
        reveal_type(x)  # revealed: str
```

## Aliases in complex predicates

```py
def _(x: int | None):
    is_none = x is None
    if bool(is_none):
        reveal_type(x)  # revealed: None
    if is_none if True else False:
        reveal_type(x)  # revealed: None
    if is_none == True:
        # TODO: it would be nice to support this case, but even direct narrowing doesn't work here
        reveal_type(x)  # revealed: int | None
    if (is_none,)[0]:
        # TODO: same as above
        reveal_type(x)  # revealed: int | None
    if y := is_none:
        reveal_type(x)  # revealed: None
        reveal_type(y)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: int
        reveal_type(y)  # revealed: Literal[False]
    if (lambda: is_none)():
        # TODO: same as above
        reveal_type(x)  # revealed: int | None
```

## Attribute access alias

```py
class A:
    x: int | None
    b: bool

    def negate_b(self):
        self.b = not self.b

def _(a: A):
    is_none = a.x is None
    if is_none:
        reveal_type(a.x)  # revealed: None
    else:
        reveal_type(a.x)  # revealed: int

def _(a: A):
    # Attribute targets are not treated as aliases.
    # It is difficult to track them accurately.
    a.b = a.x is None
    a.negate_b()
    if a.b:
        reveal_type(a.x)  # revealed: int | None
    else:
        reveal_type(a.x)  # revealed: int | None
```

## Subscript access alias

```py
def _(l: list[int | None]):
    is_none = l[0] is None
    if is_none:
        reveal_type(l[0])  # revealed: None
    else:
        reveal_type(l[0])  # revealed: int

def _(l: list[int | None], lb: list[bool]):
    # Same as attributes: subscript targets are not treated as aliases.
    lb[0] = l[0] is None
    if lb[0]:
        reveal_type(l[0])  # revealed: int | None
    else:
        reveal_type(l[0])  # revealed: int | None
```

## Narrowing is invalidated when target is reassigned

If the target is reassigned between the definition of the alias and its use as a condition,
narrowing does not take place:

```py
def _(x: int | None, cond: bool):
    is_none = x is None
    if cond:
        x = 1
    if is_none:
        reveal_type(x)  # revealed: int | None

    is_none = x is None
    if is_none:
        reveal_type(x)  # revealed: None

class A:
    x: int | None

def _(a: A):
    is_none = a.x is None
    a.x = 1
    if is_none:
        reveal_type(a.x)  # revealed: Literal[1]

def _(a: A):
    is_none = a.x is None
    a = A()
    if is_none:
        reveal_type(a.x)  # revealed: int | None

def _(x: int | None):
    # In-place reassignment
    x = x is None
    if x:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[False]
```

## Alias variable reassigned invalidates alias

If the alias variable itself is reassigned, it no longer represents the original check.

```py
def _(x: int | None):
    is_none = x is None
    is_none = True
    if is_none:
        reveal_type(x)  # revealed: int | None

    is_none = x is None
    if is_none:
        reveal_type(x)  # revealed: None
```

## Alias defined in the `if` branch

Only the `if` branch relates `condition` to `x`. After the branches merge, the independent
assignment can produce either condition outcome for any value of `x`.

```py
def _(x: int | None, flag: bool, other: bool):
    if flag:
        condition = x is not None
    else:
        condition = other

    if condition:
        reveal_type(x)  # revealed: int | None
    else:
        reveal_type(x)  # revealed: int | None
```

## Alias defined in the `else` branch

The same applies when the `else` branch assigns the check. Its relationship to `x` does not hold for
the independent assignment in the `if` branch.

```py
def _(x: int | None, flag: bool, other: bool):
    if flag:
        condition = other
    else:
        condition = x is not None

    if condition:
        reveal_type(x)  # revealed: int | None
    else:
        reveal_type(x)  # revealed: int | None
```

## Alias assigned conditionally

If the branch is skipped, `condition` keeps the independent argument value. Neither outcome of the
condition narrows `x`.

```py
def _(x: int | None, flag: bool, condition: bool):
    if flag:
        condition = x is not None

    if condition:
        reveal_type(x)  # revealed: int | None
    else:
        reveal_type(x)  # revealed: int | None
```

## Possibly unbound local alias

A missing local binding raises `UnboundLocalError` instead of falling back to an outer scope. If
evaluation succeeds, `condition` comes from the narrowing expression.

```py
def _(x: int | None, flag: bool):
    if flag:
        condition = x is not None

    if condition:  # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: None
```

## Class-local alias with a global fallback

An unbound class-local name falls back to the global binding. That independent boolean can produce
either condition outcome without narrowing the class-local target.

```py
condition: bool = True

def _(value: int | None, flag: bool):
    class C:
        x = value
        if flag:
            condition = x is not None

        if condition:
            reveal_type(x)  # revealed: int | None
        else:
            reveal_type(x)  # revealed: int | None
```

## Global alias assigned conditionally

If the assignment is skipped, a `global` name keeps its independent module-level value. Neither
outcome narrows the local target.

```py
condition: bool = True

def _(x: int | None, flag: bool):
    global condition
    if flag:
        condition = x is not None

    if condition:
        reveal_type(x)  # revealed: int | None
    else:
        reveal_type(x)  # revealed: int | None
```

## Alias assigned on a terminal branch

A check assigned on a branch that returns cannot describe the condition used afterward. The
independent assignment is the only one that reaches this use, so neither outcome narrows `x`.

```py
def _(x: int | None, flag: bool, other: bool):
    condition = other
    if flag:
        condition = x is not None
        return

    if condition:
        reveal_type(x)  # revealed: int | None
    else:
        reveal_type(x)  # revealed: int | None
```

## Alias assigned on the only continuing branch

If the other branch returns, the check is the only assignment that reaches the condition. Its
relationship to `x` still permits narrowing after the branches merge.

```py
def _(x: int | None, flag: bool):
    if flag:
        condition = x is not None
    else:
        return

    if condition:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: None
```

## Alias assigned on an always-taken branch

An alias assigned under a condition that is always true is definitely initialized. Both outcomes of
the alias narrow its target.

```py
def get_value() -> int | str:
    return 1

x = get_value()
if None is None:
    is_int = isinstance(x, int)

if is_int:
    reveal_type(x)  # revealed: int
else:
    reveal_type(x)  # revealed: str
```

## Alias reassignment on the false branch

If the final `check` is false, the `None` check ran and ruled out `None`. Otherwise, assigning
`True` to `value` leaves a `bool` on that path too.

```py
def _(value: bool | None, check: bool):
    if not check:
        check = value is None
    if check:
        reveal_type(value)  # revealed: bool | None
        value = True
    reveal_type(value)  # revealed: bool
```

## Alias preserved across loop iterations

The cached value and its alias are initialized together on the first iteration. Later iterations
reuse both, so the alias still narrows the cached value.

```py
def _(value: int | str):
    cached = None
    for _ in range(2):
        if cached is None:
            cached = value
            is_int = isinstance(cached, int)
        # TODO: recognize that `is_int` is initialized on the first iteration.
        if is_int:  # error: [possibly-unresolved-reference]
            reveal_type(cached)  # revealed: int
        else:
            reveal_type(cached)  # revealed: str
```

## Cached alias updated when false

A cached condition starts out false and is replaced by the `None` check whenever it is false. A
later true condition therefore implies that `x` is not `None`, even across loop iterations.

```py
def _(x: int | None):
    condition = False
    for _ in range(2):
        if not condition:
            condition = x is not None
        if condition:
            reveal_type(x)  # revealed: int
```

## Cached alias updated when true

The same reasoning applies with the outcomes reversed: a false condition can only come from the
`None` check, so it rules out `None`.

```py
def _(x: int | None):
    condition = True
    for _ in range(2):
        if condition:
            condition = x is None
        if not condition:
            reveal_type(x)  # revealed: int
```

## Alias reassigned on a loop backedge

A different assignment can reach the condition from an earlier iteration. That assignment does not
describe `x`, so the condition cannot narrow it.

```py
def _(x: int | None, flags: list[bool], other: bool):
    condition = False
    for flag in flags:
        if flag:
            condition = x is not None
        if condition:
            reveal_type(x)  # revealed: int | None
        else:
            reveal_type(x)  # revealed: int | None
        if other:
            condition = True
```

## Alias replaced by an independent loop-carried value

When `replace` is false, the value carried from the first iteration makes `condition` true on the
second iteration even if `x` is `None`. The condition therefore cannot narrow `x`.

```py
def _(x: int | None, replace: bool):
    carry = False
    for _ in range(2):
        condition = carry
        if replace:
            condition = x is not None
        if condition:
            reveal_type(x)  # revealed: int | None
        carry = not condition
```

## Nested scope can preserve alias

> TODO: This feature is not supported yet.

Aliases defined in the outer scope behave the same way across nested scope boundaries as if the
target had been directly narrowed (see also: [`conditionals/nested.md`](./conditionals/nested.md)).

In other words, in eager scope (class body, list comprehension, etc.), the alias is adopted as it
was when it entered the scope. In lazy scope (function body, etc.), the alias remains valid unless
either the target or the alias is reassigned.

```py
def _(x: int | None):
    is_none = x is None

    if is_none:
        reveal_type(x)  # revealed: None

    class EagerScope:
        if is_none:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None

        def lazy_scope():
            if is_none:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None

    def inner2():
        if is_none:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None

        class Inner2:
            if is_none:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None

class A:
    x: int | None

def _(a: A):
    a = A()
    is_none = a.x is None

    if is_none:
        reveal_type(a.x)  # revealed: None

    class Inner:
        if is_none:
            # TODO: should be `None`
            reveal_type(a.x)  # revealed: int | None

        def inner():
            if is_none:
                # TODO: should be `None`
                reveal_type(a.x)  # revealed: int | None

    def inner2():
        if is_none:
            # TODO: should be `None`
            reveal_type(a.x)  # revealed: int | None

        class Inner2:
            if is_none:
                # TODO: should be `None`
                reveal_type(a.x)  # revealed: int | None
```

## Cross-scope invalidation

### Target reassignments

If the target is reassigned inside an eager scope, narrowing does not take place within that scope.

```py
def _(x: int | None):
    is_none = x is None

    class Inner:
        x = 42
        x = 43
        if is_none:
            reveal_type(x)  # revealed: Literal[43]

        def f():
            reveal_type(x)  # revealed: int | None
            if is_none:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None

        class Inner2:
            if is_none:
                # `x` here refers to the function scope variable, not the class-scope `x`.
                # Python's name resolution skips class scopes for nested scopes, so the alias
                # remains valid.
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None

    if is_none:
        reveal_type(x)  # revealed: None
```

The same applies to a lazy scope:

```py
def _(x: int | None):
    is_none = x is None

    def inner():
        nonlocal x
        x = 42
        if is_none:
            reveal_type(x)  # revealed: Literal[42]

    if is_none:
        reveal_type(x)  # revealed: int | None

def _(x: int | None):
    is_none = x is None

    def inner():
        if is_none:
            reveal_type(x)  # revealed: int | None

        def inner2():
            if is_none:
                reveal_type(x)  # revealed: int | None

    x = 42

    inner()
```

### Alias variable reassigned

If the alias variable itself is reassigned inside an eager scope, the alias is invalidated within
that scope.

```py
def _(x: int | None):
    is_none = x is None

    class Inner:
        is_none = True
        if is_none:
            reveal_type(x)  # revealed: int | None

        class Inner2:
            # `is_none` here refers to the function scope variable, not the class-scope
            # `is_none = True`. Python's name resolution skips class scopes for nested
            # scopes, so the alias remains valid.
            if is_none:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None

    if is_none:
        reveal_type(x)  # revealed: None
```

The same applies to a lazy scope:

```py
def _(x: int | None):
    is_none = x is None

    def inner():
        nonlocal is_none
        is_none = True
        if is_none:
            reveal_type(x)  # revealed: int | None

    inner()

    if is_none:
        reveal_type(x)  # revealed: int | None

def _(x: int | None):
    is_none = x is None

    def inner():
        if is_none:
            reveal_type(x)  # revealed: int | None

        def inner2():
            if is_none:
                reveal_type(x)  # revealed: int | None

    is_none = True

    inner()
```

## Chained aliases

> TODO: This feature is not supported yet.

### Basic

```py
def _(x: int | None):
    is_none = x is None
    is_none_alias = is_none
    if is_none_alias:
        # TODO: should be `None`
        reveal_type(x)  # revealed: int | None

    class Inner:
        if is_none_alias:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None

    def inner():
        if is_none_alias:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None

def _(x: int | None):
    is_none = x is None
    is_none_alias = is_none

    x = 42

    if is_none_alias:
        reveal_type(x)  # revealed: Literal[42]
    if is_none:
        reveal_type(x)  # revealed: Literal[42]

    class Inner:
        if is_none_alias:
            reveal_type(x)  # revealed: Literal[42]
        if is_none:
            reveal_type(x)  # revealed: Literal[42]

    def inner():
        x = 42
        if is_none_alias:
            reveal_type(x)  # revealed: Literal[42]
        if is_none:
            reveal_type(x)  # revealed: Literal[42]

def _(x: int | None):
    is_none = x is None
    is_none_alias = is_none

    class Inner:
        is_none_alias = True
        if is_none_alias:
            reveal_type(x)  # revealed: int | None
        if is_none:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None

        class Inner2:
            if is_none_alias:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None
            if is_none:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None

    class Inner2:
        is_none = True
        if is_none_alias:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None
        if is_none:
            reveal_type(x)  # revealed: int | None

        class Inner3:
            if is_none_alias:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None
            if is_none:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None

    def inner():
        is_none_alias = True
        if is_none_alias:
            reveal_type(x)  # revealed: int | None
        if is_none:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None

        def inner2():
            if is_none_alias:
                reveal_type(x)  # revealed: int | None
            if is_none:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None

    def inner2():
        is_none = True
        if is_none_alias:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None
        if is_none:
            reveal_type(x)  # revealed: int | None

        def inner3():
            if is_none_alias:
                # TODO: should be `None`
                reveal_type(x)  # revealed: int | None
            if is_none:
                reveal_type(x)  # revealed: int | None
```

### Cross-scope chained alias

```py
def _(x: int | None):
    is_none = x is None

    class Inner:
        is_none_alias = is_none
        if is_none_alias:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None

    def inner():
        is_none_alias = is_none
        if is_none_alias:
            # TODO: should be `None`
            reveal_type(x)  # revealed: int | None

is_none = True

def _(x: int | None):
    is_none = x is None

    class Inner:
        # This resolves to the global `is_none`!
        is_none_alias = is_none
        is_none = False
        reveal_type(is_none_alias)  # revealed: Literal[True]
        if is_none_alias:
            reveal_type(x)  # revealed: int | None

    def inner():
        # error: [unresolved-reference] "Name `is_none` used when not defined"
        is_none_alias = is_none
        is_none = True
        if is_none_alias:
            reveal_type(x)  # revealed: int | None

def _(x: int | None):
    is_none = x is None

    class Inner:
        is_none_alias = is_none
        x = 42
        if is_none_alias:
            reveal_type(x)  # revealed: Literal[42]

    def inner():
        is_none_alias = is_none
        x = 42
        if is_none_alias:
            reveal_type(x)  # revealed: Literal[42]
```

### Negated chained alias

```py
def _(x: int | None):
    is_none = x is None
    is_not_none = not is_none
    if is_not_none:
        # TODO: should be `int`
        reveal_type(x)  # revealed: int | None

    class Inner:
        if is_not_none:
            # TODO: should be `int`
            reveal_type(x)  # revealed: int | None

    def inner():
        if is_not_none:
            # TODO: should be `int`
            reveal_type(x)  # revealed: int | None

def _(x: int | None):
    is_none = x is None
    is_not_none = not is_none
    if is_not_none:
        # TODO: should be `int`
        reveal_type(x)  # revealed: int | None

    class Inner:
        x = 42
        if is_not_none:
            reveal_type(x)  # revealed: Literal[42]

    def inner():
        x = 42
        if is_not_none:
            reveal_type(x)  # revealed: Literal[42]

def _(x: int | None):
    is_none = x is None
    is_not_none = not is_none

    is_none = True
    if is_not_none:
        # TODO: should be `int`
        reveal_type(x)  # revealed: int | None

    class Inner:
        is_none = True
        if is_not_none:
            # TODO: should be `int`
            reveal_type(x)  # revealed: int | None

    def inner():
        is_none = True
        if is_not_none:
            # TODO: should be `int`
            reveal_type(x)  # revealed: int | None
```

### Boolean-operated chained alias

```py
def _(x: int | None):
    is_none = x is None
    is_int = isinstance(x, int)
    is_none_and_int = is_none and is_int
    if is_none_and_int:
        # TODO: should be `Never`
        reveal_type(x)  # revealed: int | None

    class Inner:
        if is_none_and_int:
            # TODO: should be `Never`
            reveal_type(x)  # revealed: int | None

    def inner():
        if is_none_and_int:
            # TODO: should be `Never`
            reveal_type(x)  # revealed: int | None

def _(x: str | int | None):
    is_none = x is None
    is_int = isinstance(x, int)
    is_int_or_none = is_int or is_none
    if is_int_or_none:
        # TODO: should be `int | None`
        reveal_type(x)  # revealed: str | int | None

    class Inner:
        if is_int_or_none:
            # TODO: should be `int | None`
            reveal_type(x)  # revealed: str | int | None

    def inner():
        if is_int_or_none:
            # TODO: should be `int | None`
            reveal_type(x)  # revealed: str | int | None
```

## Simple name aliases do not have a narrowing effect

This is a technical limitation: simple name aliases are so common in real-world Python code that
assuming all of them are subject to alias narrowing would lead to performance degradation. TODO: It
would be nice if we could resolve this limitation, but it probably won't be a serious issue in
practice.

```py
def _(x: int, y: bool):
    if x:
        reveal_type(x)  # revealed: int & ~AlwaysFalsy
    if y:
        reveal_type(y)  # revealed: Literal[True]
    if x and y:
        reveal_type(x)  # revealed: int & ~AlwaysFalsy
        reveal_type(y)  # revealed: Literal[True]

    x_alias = x
    y_alias = y
    if x_alias:
        reveal_type(x)  # revealed: int
    if y_alias:
        reveal_type(y)  # revealed: bool
    if x_alias and y_alias:
        reveal_type(x)  # revealed: int
        reveal_type(y)  # revealed: bool

    x_alias2 = bool(x)
    y_alias2 = bool(y)
    if x_alias2:
        reveal_type(x)  # revealed: int & ~AlwaysFalsy
    if y_alias2:
        reveal_type(y)  # revealed: Literal[True]
    if x_alias2 and y_alias2:
        reveal_type(x)  # revealed: int & ~AlwaysFalsy
        reveal_type(y)  # revealed: Literal[True]
```

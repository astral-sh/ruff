# Public types

## Basic

The "public type" of a symbol refers to the type that is inferred in a nested scope for a symbol
defined in an outer enclosing scope. Since it is not generally possible to analyze the full control
flow of a program, we currently make the simplifying assumption that an inner scope (such as the
`inner` function below) could be executed at any position in the enclosing scope. The public type
should therefore be the union of all possible types that the symbol could have.

In the following example, depending on when `inner()` is called, the type of `x` could either be `A`
or `B`:

```py
class A: ...
class B: ...
class C: ...

def outer() -> None:
    x = A()

    def inner() -> None:
        # TODO: We might ideally be able to eliminate `Unknown` from the union here since `x` resolves to an
        # outer scope that is a function scope (as opposed to module global scope), and `x` is never declared
        # nonlocal in a nested scope that also assigns to it.
        reveal_type(x)  # revealed: Unknown | A | B
    # This call would observe `x` as `A`.
    inner()

    x = B()

    # This call would observe `x` as `B`.
    inner()
```

Similarly, if control flow in the outer scope can split, the public type of `x` should reflect that:

```py
def outer(flag: bool) -> None:
    x = A()

    def inner() -> None:
        reveal_type(x)  # revealed: Unknown | A | B | C
    inner()

    if flag:
        x = B()

        inner()
    else:
        x = C()

        inner()

    inner()
```

If a binding is not reachable, it is not considered in the public type:

```py
def outer() -> None:
    x = A()

    def inner() -> None:
        reveal_type(x)  # revealed: Unknown | A | C
    inner()

    if False:
        x = B()  # this binding of `x` is unreachable
        inner()

    x = C()
    inner()

def outer(flag: bool) -> None:
    x = A()

    def inner() -> None:
        reveal_type(x)  # revealed: Unknown | A | C
    inner()

    if flag:
        return

        x = B()  # this binding of `x` is unreachable

    x = C()
    inner()
```

If a symbol is only conditionally bound, we do not raise any errors:

```py
def outer(flag: bool) -> None:
    if flag:
        x = A()

        def inner() -> None:
            reveal_type(x)  # revealed: Unknown | A
        inner()
```

In the future, we may try to be smarter about which bindings must or must not be a visible to a
given nested scope, depending where it is defined. In the above case, this shouldn't change the
behavior -- `x` is defined before `inner` in the same branch, so should be considered
definitely-bound for `inner`. But in other cases we may want to emit `possibly-unresolved-reference`
in future:

```py
def outer(flag: bool) -> None:
    if flag:
        x = A()

    def inner() -> None:
        # TODO: Ideally, we would emit a possibly-unresolved-reference error here.
        reveal_type(x)  # revealed: Unknown | A
    inner()
```

The public type is available, even if the end of the outer scope is unreachable. This is a
regression test. A previous version of ty used the end-of-scope position to determine the public
type, which would have resulted in incorrect type inference here:

```py
def outer() -> None:
    x = A()

    def inner() -> None:
        reveal_type(x)  # revealed: Unknown | A
    inner()

    return
    # unreachable

def outer(flag: bool) -> None:
    x = A()

    def inner() -> None:
        reveal_type(x)  # revealed: Unknown | A | B
    if flag:
        x = B()
        inner()
        return
        # unreachable

    inner()

def outer(x: A) -> None:
    def inner() -> None:
        reveal_type(x)  # revealed: A
    raise
```

An arbitrary level of nesting is supported:

```py
def f0() -> None:
    x = A()

    def f1() -> None:
        def f2() -> None:
            def f3() -> None:
                def f4() -> None:
                    reveal_type(x)  # revealed: Unknown | A | B
                f4()
            f3()
        f2()
    f1()

    x = B()

    f1()
```

## At module level

The behavior is the same if the outer scope is the global scope of a module:

```py
def flag() -> bool:
    return True

if flag():
    x = 1

    def f() -> None:
        reveal_type(x)  # revealed: Unknown | Literal[1, 2]
    # Function only used inside this branch
    f()

    x = 2

    # Function only used inside this branch
    f()
```

## Mixed declarations and bindings

When a declaration only appears in one branch, we also consider the types of the symbol's bindings
in other branches:

```py
def flag() -> bool:
    return True

if flag():
    A: str = ""
else:
    A = None

reveal_type(A)  # revealed: Literal[""] | None

def _():
    reveal_type(A)  # revealed: str | None
```

This pattern appears frequently with conditional imports. The `import` statement is both a
declaration and a binding, but we still add `None` to the public type union in a situation like
this:

```py
try:
    import optional_dependency  # ty: ignore
except ImportError:
    optional_dependency = None

reveal_type(optional_dependency)  # revealed: Unknown | None

def _():
    reveal_type(optional_dependency)  # revealed: Unknown | None
```

## Limitations

### Type narrowing

We currently do not further analyze control flow, so we do not support cases where the inner scope
is only executed in a branch where the type of `x` is narrowed:

```py
class A: ...

def outer(x: A | None):
    if x is not None:
        def inner() -> None:
            # TODO: should ideally be `A`
            reveal_type(x)  # revealed: A | None
        inner()
```

### Shadowing

Similarly, since we do not analyze control flow in the outer scope here, we assume that `inner()`
could be called between the two assignments to `x`:

```py
def outer() -> None:
    def inner() -> None:
        # TODO: this should ideally be `Unknown | Literal[1]`, but no other type checker supports this either
        reveal_type(x)  # revealed: Unknown | None | Literal[1]
    x = None

    # [additional code here]

    x = 1

    inner()
```

This is currently even true if the `inner` function is only defined after the second assignment to
`x`:

```py
def outer() -> None:
    x = None

    # [additional code here]

    x = 1

    def inner() -> None:
        # TODO: this should be `Unknown | Literal[1]`. Mypy and pyright support this.
        reveal_type(x)  # revealed: Unknown | None | Literal[1]
    inner()
```

A similar case derived from an ecosystem example, involving declared types:

```py
class C: ...

def outer(x: C | None):
    x = x or C()

    reveal_type(x)  # revealed: C

    def inner() -> None:
        # TODO: this should ideally be `C`
        reveal_type(x)  # revealed: C | None
    inner()
```

### Assignments to nonlocal variables

Writes to the outer-scope variable are currently not detected:

```py
def outer() -> None:
    x = None

    def set_x() -> None:
        nonlocal x
        x = 1
    set_x()

    def inner() -> None:
        # TODO: this should ideally be `Unknown | None | Literal[1]`. Mypy and pyright support this.
        reveal_type(x)  # revealed: Unknown | None
    inner()
```

## Handling of overloads

### With implementation

Overloads need special treatment, because here, we do not want to consider *all* possible
definitions of `f`. This would otherwise result in a union of all three definitions of `f`:

```py
from typing import overload

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
def f(x: int | str) -> int | str:
    raise NotImplementedError

reveal_type(f)  # revealed: Overload[(x: int) -> int, (x: str) -> str]

def _():
    reveal_type(f)  # revealed: Overload[(x: int) -> int, (x: str) -> str]
```

This also works if there are conflicting declarations:

```py
def flag() -> bool:
    return True

if flag():
    @overload
    def g(x: int) -> int: ...
    @overload
    def g(x: str) -> str: ...
    def g(x: int | str) -> int | str:
        return x

else:
    g: str = ""

def _():
    reveal_type(g)  # revealed: (Overload[(x: int) -> int, (x: str) -> str]) | str

# error: [conflicting-declarations]
g = "test"
```

### Without an implementation

Similarly, if there is no implementation, we only consider the last overload definition.

```pyi
from typing import overload

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...

reveal_type(f)  # revealed: Overload[(x: int) -> int, (x: str) -> str]

def _():
    reveal_type(f)  # revealed: Overload[(x: int) -> int, (x: str) -> str]
```

This also works if there are conflicting declarations:

```pyi
def flag() -> bool:
    return True

if flag():
    @overload
    def g(x: int) -> int: ...
    @overload
    def g(x: str) -> str: ...
else:
    g: str

def _():
    reveal_type(g)  # revealed: (Overload[(x: int) -> int, (x: str) -> str]) | str
```

### Overload only defined in one branch

```py
from typing import overload

def flag() -> bool:
    return True

if flag():
    @overload
    def f(x: int) -> int: ...
    @overload
    def f(x: str) -> str: ...
    def f(x: int | str) -> int | str:
        raise NotImplementedError

    def _():
        reveal_type(f)  # revealed: Overload[(x: int) -> int, (x: str) -> str]
```

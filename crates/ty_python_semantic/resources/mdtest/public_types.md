# Public types

## Basic

The "public type" of a symbol refers to the type that is inferred for a symbol from another scope.
Since it is not generally possible to analyze the full control flow of a program, we currently make
the assumption that the inner scope (such as the inner function below) could be executed at any
position. The public type should therefore be the union of all possible types that the symbol could
have.

In the following example, depending on when `inner()` is called, the type of `x` could either be `A`
or `B`:

```py
class A: ...
class B: ...
class C: ...

def outer() -> None:
    x = A()

    def inner() -> None:
        reveal_type(x)  # revealed: Unknown | A | B
    inner()

    x = B()

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
        x = B()
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

        x = B()

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

If a symbol is possibly unbound, we do not currently attempt to detect this:

```py
def outer(flag: bool) -> None:
    if flag:
        x = A()

    def inner() -> None:
        # TODO: Ideally, we would emit a possibly-unresolved-reference error here.
        reveal_type(x)  # revealed: Unknown | A
    inner()
```

The public type is available even if the end of the outer scope is unreachable:

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
```

This works at arbitrary levels of nesting:

```py
def outer() -> None:
    x = A()

    def intermediate() -> None:
        def inner() -> None:
            reveal_type(x)  # revealed: Unknown | A | B
        inner()
    intermediate()

    x = B()

    intermediate()

def outer(x: A) -> None:
    def inner() -> None:
        reveal_type(x)  # revealed: A
    raise
```

## Interplay with type narrowing

```py
class A: ...

def outer(x: A | None):
    def inner() -> None:
        reveal_type(x)  # revealed: A | None
    inner()
    if x is None:
        inner()

def outer(x: A | None):
    if x is not None:
        def inner() -> None:
            # TODO: should ideally be `A`
            reveal_type(x)  # revealed: A | None
        inner()
```

## At module level

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

## Limitations

```py
def outer():
    x = None

    # […]

    x = 1

    def inner():
        # TODO: this should ideally be `Unknown | Literal[1]`
        reveal_type(x)  # revealed: Unknown | None | Literal[1]
    inner()
```

## Overloads

```py
from typing import overload

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
def f(x: int | str) -> int | str:
    raise NotImplementedError

reveal_type(f(1))  # revealed: int
reveal_type(f("a"))  # revealed: str

def _():
    reveal_type(f(1))  # revealed: int
    reveal_type(f("a"))  # revealed: str
```

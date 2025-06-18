# Public types

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
        # TODO: should be `Unknown | A | B`
        reveal_type(x)  # revealed: Unknown | B
    inner()

    x = B()

    inner()
```

Similarly, if control flow in the outer scope can split, the public type of `x` should reflect that:

```py
def outer(flag: bool) -> None:
    x = A()

    def inner() -> None:
        # TODO: should be `Unknown | A | B | C`
        reveal_type(x)  # revealed: Unknown | B | C
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
def outer(flag: bool) -> None:
    x = A()

    def inner() -> None:
        # TODO: should be `Unknown | A | C`
        reveal_type(x)  # revealed: Unknown | C
    if False:
        x = B()
        inner()

    x = C()
    inner()
```

If a symbol is only conditionally bound, we do not raise any errors:

```py
def outer(flag: bool) -> None:
    if flag:
        x = A()

        def inner() -> None:
            # TODO: this should not be an error
            # error: [possibly-unresolved-reference]
            reveal_type(x)  # revealed: Unknown | A
        inner()
```

If a symbol is possibly unbound, we do not currently attempt to detect this:

```py
def outer(flag: bool) -> None:
    if flag:
        x = A()

    def inner() -> None:
        # TODO: currently an error (good), but this diagnostic might go away if
        # we try to silence the one above.
        # error: [possibly-unresolved-reference]
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
        # TODO: should be `Unknown | A | B`
        reveal_type(x)  # revealed: Unknown | A
    if flag:
        x = B()
        inner()
        return
        # unreachable

    inner()
```

The same set of problems can appear at module scope:

```py
def flag() -> bool:
    return True

if flag():
    x = 1

    def f() -> None:
        # TODO: no error, type should be `Unknown | Literal[1, 2]`
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Unknown | Literal[2]
    # Function only used inside this branch
    f()

    x = 2

    # Function only used inside this branch
    f()
```

# Nonlocal references

## One level up

```py
def f():
    x = 1
    def g():
        reveal_type(x)  # revealed: Unknown | Literal[1]
```

## Two levels up

```py
def f():
    x = 1
    def g():
        def h():
            reveal_type(x)  # revealed: Unknown | Literal[1]
```

## Skips class scope

```py
def f():
    x = 1

    class C:
        x = 2
        def g():
            reveal_type(x)  # revealed: Unknown | Literal[1]
```

## Skips annotation-only assignment

```py
def f():
    x = 1
    def g():
        # it's pretty weird to have an annotated assignment in a function where the
        # name is otherwise not defined; maybe should be an error?
        x: int
        def h():
            reveal_type(x)  # revealed: Unknown | Literal[1]
```

## The `nonlocal` keyword

Without the `nonlocal` keyword, `x += 1` is not allowed in an inner scope, even if we break it up
into multiple steps. Local variable scoping is "forward-looking" in the sense that even a _later_
assignment of `x` means that _all_ reads of `x` in that scope only look at that scope's binding:

```py
def f():
    x = 1
    def g():
        x += 1  # error: [unresolved-reference]

def f():
    x = 1
    def g():
        y = x  # error: [unresolved-reference]
        x = y + 1
```

With `nonlocal` these examples work, because the reassignments modify the variable from the outer
scope rather than modifying a separate variable local to the inner scope:

```py
def f():
    x = 1
    def g():
        nonlocal x
        x += 1
        reveal_type(x)  # revealed: Unknown | Literal[2]

def f():
    x = 1
    def g():
        nonlocal x
        y = x
        x = y + 1
        reveal_type(x)  # revealed: Unknown | Literal[2]

def f():
    x = 1
    y = 2
    def g():
        nonlocal x, y
        reveal_type(x)  # revealed: Unknown | Literal[1]
        reveal_type(y)  # revealed: Unknown | Literal[2]
```

## `nonlocal` declarations must match an outer binding

`nonlocal x` isn't allowed when there's no binding for `x` in an enclosing scope:

```py
def f():
    def g():
        nonlocal x  # error: [invalid-nonlocal] "no binding for nonlocal `x` found"

def f():
    x = 1
    def g():
        nonlocal x, y  # error: [invalid-nonlocal] "no binding for nonlocal `y` found"
```

A global `x` doesn't work either; the outer-scope binding for the variable has to originate from a
function-like scope:

```py
x = 1

def f():
    def g():
        nonlocal x  # error: [invalid-nonlocal] "no binding for nonlocal `x` found"
```

A class-scoped `x` also doesn't work, for the same reason:

```py
class Foo:
    x = 1
    @staticmethod
    def f():
        nonlocal x  # error: [invalid-nonlocal] "no binding for nonlocal `x` found"
```

## `nonlocal` uses the closest binding

```py
def f():
    x = 1
    def g():
        x = 2
        def h():
            nonlocal x
            reveal_type(x)  # revealed: Unknown | Literal[2]
```

## `nonlocal` "chaining"

Multiple `nonlocal` statements can "chain" through nested scopes:

```py
def f():
    x = 1
    def g():
        nonlocal x
        def h():
            nonlocal x
            reveal_type(x)  # revealed: Unknown | Literal[1]
```

And the `nonlocal` chain can skip over a scope that doesn't bind the variable:

```py
def f1():
    x = 1
    def f2():
        nonlocal x
        def f3():
            # No binding; this scope gets skipped.
            def f4():
                nonlocal x
                reveal_type(x)  # revealed: Unknown | Literal[1]
```

But a `global` statement breaks the chain:

```py
def f():
    x = 1
    def g():
        global x
        def h():
            nonlocal x  # error: [invalid-nonlocal] "no binding for nonlocal `x` found"
```

## `nonlocal` bindings respect declared types from the defining scope

By default (without `nonlocal`), an inner variable shadows an outer variable of the same name, and
type declarations from the outer scope don't apply to the inner one:

```py
def f():
    x: int = 1
    def g():
        # `Literal["string"]` is not assignable to `int` # (the declared type in the outer scope),
        # but we don't emit a diagnostic complaining about it because `x` in the inner scope is a
        # distinct variable; the outer-scope declarations do not apply to it.
        x = "string"
```

But when `x` is `nonlocal`, type declarations from the defining scope apply to it:

```py
def f():
    x: int = 1
    def g():
        nonlocal x
        x = "string"  # error: [invalid-assignment] "Object of type `Literal["string"]` is not assignable to `int`"
```

This is true even if the outer scope declares `x` without binding it:

```py
def f():
    x: int
    def g():
        nonlocal x
        x = "string"  # error: [invalid-assignment] "Object of type `Literal["string"]` is not assignable to `int`"
```

We can "see through" multiple layers of `nonlocal` statements, and also through scopes that don't
bind the variable at all. However, we don't see through `global` statements:

```py
def f1():
    # The original bindings of `x`, `y`, and `z` with type declarations.
    x: int = 1
    y: int = 1
    z: int = 1

    def f2():
        # This scope doesn't touch `x`, `y`, or `z` at all.

        def f3():
            # This scope treats declares `x` nonlocal and `y` as global, and it shadows `z` without
            # giving it a new type declaration.
            nonlocal x
            x = 2
            global y
            y = 2
            z = 2

            def f4():
                # This scope sees `x` from `f1` and `z` from `f3`, but it doesn't see `y` at all,
                # because of the `global` keyword above.
                nonlocal x, y, z  # error: [invalid-nonlocal] "no binding for nonlocal `y` found"
                x = "string"  # error: [invalid-assignment]
                z = "string"  # not an error
```

## TODO: `nonlocal` affects the inferred type in the outer scope

Without `nonlocal`, `g` can't write to `x`, and the inferred type of `x` in `f`'s scope isn't
affected by `g`:

```py
def f():
    x = 1
    def g():
        reveal_type(x)  # revealed: Unknown | Literal[1]
    reveal_type(x)  # revealed: Literal[1]
```

But with `nonlocal`, `g` could write to `x`, and that affects its inferred type in `f`. That's true
regardless of whether `g` actually writes to `x`. With a write:

```py
def f():
    x = 1
    def g():
        nonlocal x
        reveal_type(x)  # revealed: Unknown | Literal[1]
        x += 1
        reveal_type(x)  # revealed: Unknown | Literal[2]
    # TODO: should be `Unknown | Literal[1]`
    reveal_type(x)  # revealed: Literal[1]
```

Without a write:

```py
def f():
    x = 1
    def g():
        nonlocal x
        reveal_type(x)  # revealed: Unknown | Literal[1]
    # TODO: should be `Unknown | Literal[1]`
    reveal_type(x)  # revealed: Literal[1]
```

## Annotating a `nonlocal` binding is a syntax error

```py
def f():
    x: int = 1
    def g():
        nonlocal x
        x: str = "foo"  # error: [invalid-syntax] "annotated name `x` can't be nonlocal"
```

## `nonlocal` after use

Using a name prior to its `nonlocal` declaration in the same scope is a syntax error:

```py
def f():
    x = 1
    def g():
        print(x)
        nonlocal x  # error: [invalid-syntax] "name `x` is used prior to nonlocal declaration"
```

## `nonlocal` before outer initialization

`nonlocal x` works even if `x` isn't bound in the enclosing scope until afterwards (since the
function defining the inner scope might only be called after the later binding!):

```py
def f():
    def g():
        def h():
            nonlocal x
            reveal_type(x)  # revealed: Unknown | Literal[1]
        x = 1
```

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

Without the `nonlocal` keyword, bindings in an inner scope shadow variables of the same name in
enclosing scopes. This example isn't a type error, because the inner `x` shadows the outer one:

```py
def f():
    x: int = 1
    def g():
        x = "hello"  # allowed
```

With `nonlocal` it is a type error, because `x` refers to the same place in both scopes:

```py
def f():
    x: int = 1
    def g():
        nonlocal x
        x = "hello"  # error: [invalid-assignment] "Object of type `Literal["hello"]` is not assignable to `int`"
```

## Local variable bindings "look ahead" to any assignment in the current scope

The binding `x = 2` in `g` causes the earlier read of `x` to refer to `g`'s not-yet-initialized
binding, rather than to `x = 1` in `f`'s scope:

```py
def f():
    x = 1
    def g():
        if x == 1:  # error: [unresolved-reference] "Name `x` used when not defined"
            x = 2
```

The `nonlocal` keyword makes this example legal (and makes the assignment `x = 2` affect the outer
scope):

```py
def f():
    x = 1
    def g():
        nonlocal x
        if x == 1:
            x = 2
```

For the same reason, using the `+=` operator in an inner scope is an error without `nonlocal`
(unless you shadow the outer variable first):

```py
def f():
    x = 1
    def g():
        x += 1  # error: [unresolved-reference] "Name `x` used when not defined"

def f():
    x = 1
    def g():
        x = 1
        x += 1  # allowed, but doesn't affect the outer scope

def f():
    x = 1
    def g():
        nonlocal x
        x += 1  # allowed, and affects the outer scope
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

A global `x` doesn't work. The target must be in a function-like scope:

```py
x = 1

def f():
    def g():
        nonlocal x  # error: [invalid-nonlocal] "no binding for nonlocal `x` found"

def f():
    global x
    def g():
        nonlocal x  # error: [invalid-nonlocal] "no binding for nonlocal `x` found"
```

A class-scoped `x` also doesn't work:

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

## `nonlocal` bindings respect declared types from the defining scope, even without a binding

```py
def f():
    x: int
    def g():
        nonlocal x
        x = "string"  # error: [invalid-assignment] "Object of type `Literal["string"]` is not assignable to `int`"
```

## A complicated mixture of `nonlocal` chaining, empty scopes, and the `global` keyword

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

## Use before `nonlocal`

Using a name prior to its `nonlocal` declaration in the same scope is a syntax error:

```py
def f():
    x = 1
    def g():
        x = 2
        nonlocal x  # error: [invalid-syntax] "name `x` is used prior to nonlocal declaration"
```

This is true even if there are multiple `nonlocal` declarations of the same variable, as long as any
of them come after the usage:

```py
def f():
    x = 1
    def g():
        nonlocal x
        x = 2
        nonlocal x  # error: [invalid-syntax] "name `x` is used prior to nonlocal declaration"

def f():
    x = 1
    def g():
        nonlocal x
        nonlocal x
        x = 2  # allowed
```

## `nonlocal` before outer initialization

`nonlocal x` works even if `x` isn't bound in the enclosing scope until afterwards:

```py
def f():
    def g():
        # This is allowed, because of the subsequent definition of `x`.
        nonlocal x
    x = 1
```

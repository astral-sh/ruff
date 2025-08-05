# Nonlocal references

## One level up

```py
def f():
    x = 1
    def g():
        reveal_type(x)  # revealed: Literal[1]
```

## Two levels up

```py
def f():
    x = 1
    def g():
        def h():
            reveal_type(x)  # revealed: Literal[1]
```

## Skips class scope

```py
def f():
    x = 1

    class C:
        x = 2
        def g():
            reveal_type(x)  # revealed: Literal[1]
```

## Reads respect annotation-only declarations

```py
def f():
    x: int = 1
    def g():
        # TODO: This example should actually be an unbound variable error. However to avoid false
        # positives, we'd need to analyze `nonlocal x` statements in other inner functions.
        x: str
        def h():
            reveal_type(x)  # revealed: str
```

## Reads terminate at the `global` keyword in an enclosing scope, even if there's no binding in that scope

_Unlike_ variables that are explicitly declared `nonlocal` (below), implicitly nonlocal ("free")
reads can come from a variable that's declared `global` in an enclosing scope. It doesn't matter
whether the variable is bound in that scope:

```py
x: int = 1

def f():
    x: str = "hello"
    def g():
        global x
        def h():
            # allowed: this loads the global `x` variable due to the `global` declaration in the immediate enclosing scope
            y: int = x
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

## The types of `nonlocal` binding get unioned

Without a type declaration, we union the bindings in enclosing scopes to infer a type. But name
resolution stops at the closest binding that isn't declared `nonlocal`, and we ignore bindings
outside of that one:

```py
def a():
    # This binding is shadowed in `b`, so we ignore it in inner scopes.
    x = 1

    def b():
        x = 2

        def c():
            nonlocal x
            x = 3

            def d():
                nonlocal x
                reveal_type(x)  # revealed: Literal[3, 2]
                x = 4
                reveal_type(x)  # revealed: Literal[4]

                def e():
                    reveal_type(x)  # revealed: Literal[4, 3, 2]
```

However, currently the union of types that we build is incomplete. We walk parent scopes, but not
sibling scopes, child scopes, second-cousin-once-removed scopes, etc:

```py
def a():
    x = 1
    def b():
        nonlocal x
        x = 2

    def c():
        def d():
            nonlocal x
            x = 3
        # TODO: This should include 2 and 3.
        reveal_type(x)  # revealed: Literal[1]
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
        nonlocal x  # error: [invalid-syntax] "no binding for nonlocal `x` found"

def f():
    x = 1
    def g():
        nonlocal x, y  # error: [invalid-syntax] "no binding for nonlocal `y` found"
```

A global `x` doesn't work. The target must be in a function-like scope:

```py
x = 1

def f():
    def g():
        nonlocal x  # error: [invalid-syntax] "no binding for nonlocal `x` found"

def f():
    global x
    def g():
        nonlocal x  # error: [invalid-syntax] "no binding for nonlocal `x` found"

def f():
    # A *use* of `x` in an enclosing scope isn't good enough. There needs to be a binding.
    print(x)
    def g():
        nonlocal x  # error: [invalid-syntax] "no binding for nonlocal `x` found"
```

A class-scoped `x` also doesn't work:

```py
class Foo:
    x = 1
    @staticmethod
    def f():
        nonlocal x  # error: [invalid-syntax] "no binding for nonlocal `x` found"
```

However, class-scoped bindings don't break the `nonlocal` chain the way `global` declarations do:

```py
def f():
    x: int = 1

    class Foo:
        x: str = "hello"

        @staticmethod
        def g():
            # Skips the class scope and reaches the outer function scope.
            nonlocal x
            x = 2  # allowed
            x = "goodbye"  # error: [invalid-assignment]
```

## `nonlocal` uses the closest binding

```py
def f():
    x = 1
    def g():
        x = 2
        def h():
            nonlocal x
            reveal_type(x)  # revealed: Literal[2]
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
            reveal_type(x)  # revealed: Literal[1]
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
                reveal_type(x)  # revealed: Literal[1]
```

But a `global` statement breaks the chain:

```py
x = 1

def f():
    x = 2
    def g():
        global x
        def h():
            nonlocal x  # error: [invalid-syntax] "no binding for nonlocal `x` found"
```

## Assigning to a `nonlocal` respects the declared type from its defining scope, even without a binding in that scope

```py
def f():
    x: int
    def g():
        nonlocal x
        x = "string"  # error: [invalid-assignment] "Object of type `Literal["string"]` is not assignable to `int`"
```

## A complicated mixture of `nonlocal` chaining, empty scopes, class scopes, and the `global` keyword

```py
# Global definitions of `x`, `y`, and `z`.
x: bool = True
y: bool = True
z: bool = True

def f1():
    # Local definitions of `x`, `y`, and `z`.
    x: int = 1
    y: int = 2
    z: int = 3

    def f2():
        # This scope doesn't touch `x`, `y`, or `z` at all.

        class Foo:
            # This class scope is totally ignored.
            x: str = "a"
            y: str = "b"
            z: str = "c"

            @staticmethod
            def f3():
                # This scope declares `x` nonlocal, shadows `y` without a type declaration, and
                # declares `z` global.
                nonlocal x
                x = 4
                y = 5
                global z

                def f4():
                    # This scope sees `x` from `f1` and `y` from `f3`. It *can't* declare `z`
                    # nonlocal, because of the global statement above, but it *can* load `z` as a
                    # "free" variable, in which case it sees the global value.
                    nonlocal x, y, z  # error: [invalid-syntax] "no binding for nonlocal `z` found"
                    x = "string"  # error: [invalid-assignment]
                    y = "string"  # allowed, because `f3`'s `y` is untyped
```

## TODO: `nonlocal` affects the inferred type in the outer scope

Without `nonlocal`, `g` can't write to `x`, and the inferred type of `x` in `f`'s scope isn't
affected by `g`:

```py
def f():
    x = 1
    def g():
        reveal_type(x)  # revealed: Literal[1]
    reveal_type(x)  # revealed: Literal[1]
```

But with `nonlocal`, `g` could write to `x`, and that affects its inferred type in `f`. That's true
regardless of whether `g` actually writes to `x`. With a write:

```py
def f():
    x = 1
    def g():
        nonlocal x
        reveal_type(x)  # revealed: Literal[1]
        x += 1
        reveal_type(x)  # revealed: Literal[2]
    # TODO: should be `Unknown | Literal[1]`
    reveal_type(x)  # revealed: Literal[1]
```

Without a write:

```py
def f():
    x = 1
    def g():
        nonlocal x
        reveal_type(x)  # revealed: Literal[1]
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

## Narrowing nonlocal types to `Never` doesn't make them unbound

```py
def foo():
    x: int = 1
    def bar():
        if isinstance(x, str):
            reveal_type(x)  # revealed: Never
```

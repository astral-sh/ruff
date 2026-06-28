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

Class-scoped symbols aren't visible to nested function-scoped reads, whether they're explicitly
`nonlocal` or not.

```py
def f():
    x = 1

    class C:
        x = 2

        def g():
            reveal_type(x)  # revealed: Literal[1]

        def h():
            nonlocal x
            reveal_type(x)  # revealed: Literal[1]
```

A `nonlocal` write also doesn't affect the inferred type of a class-scoped symbol:

```py
def f():
    x = 1

    class C:
        x = 2
        def g():
            nonlocal x
            x = 3
        reveal_type(x)  # revealed: Literal[2]

    class D:
        def g():
            nonlocal x
            x = 3
        # With no local binding in the class body, the function-scoped `x` is visible.
        reveal_type(x)  # revealed: Literal[3, 1]
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

## Nested function after conditional nonlocal rebinding

An inner function should still resolve a name through an enclosing `nonlocal` declaration, even if
that enclosing scope also conditionally rebinds the name:

```py
def outer(flag: bool) -> None:
    x: int = 1

    def middle() -> None:
        nonlocal x

        if flag:
            x = 2
            return

        def inner() -> None:
            y: int = x
```

## Generator expression after nonlocal rebinding

A nested eager scope such as a generator expression should see the rebound type of a `nonlocal`
symbol:

```py
from typing import Optional

class C:
    value: int

def check(x: Optional[C]) -> C:
    return C()

def outer(x: Optional[C]) -> None:
    def inner() -> None:
        nonlocal x
        x = check(x)
        all(reveal_type(x.value) == 1 for _ in [0])  # revealed: int
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
                # Note that `d` could be called more than once, so the assignment of 4 below is
                # already potentially visible. The rules for this are subtle and in fact
                # intentionally unsound. See "Visibility of `global` and `nonlocal` bindings from
                # nested and sibling scopes" below for the details.
                reveal_type(x)  # revealed: Literal[2, 4, 3]
                x = 4
                reveal_type(x)  # revealed: Literal[4]

                def e():
                    reveal_type(x)  # revealed: Literal[2, 4, 3]
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

## `nonlocal` declarations in class scopes are also validated

```py
class C:
    nonlocal x  # error: [invalid-syntax] "no binding for nonlocal `x` found"

def f():
    class C:
        nonlocal x  # error: [invalid-syntax] "no binding for nonlocal `x` found"

def g():
    x = 1
    class C:
        nonlocal x  # ok
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

## Narrowing nonlocal variables with conditional assignments

When a nonlocal variable is conditionally reassigned and then narrowed via an assertion, the
narrowing constraint should be applied correctly, even when the enclosing scope's type was itself
narrowed (e.g., via an `isinstance` check).

```py
def _(maybe_float: float | None, certain_int: int, flag: bool) -> None:
    if isinstance(maybe_float, int):
        return
    x = maybe_float
    def _() -> None:
        nonlocal x
        if flag:
            x = certain_int
        assert x is not None
        reveal_type(x)  # revealed: int | float
        +x
```

## Visibility of `nonlocal` bindings from nested and sibling scopes

(`global` bindings behave similarly and have a similarly named test case in `global.md`.)

A `nonlocal` write from an inner scope can affect the target variable's inferred type in it's
defining scope. For the same reason, reads in a nested function can also see later bindings in their
own scope:

```py
def f():
    x = 1
    def g():
        nonlocal x
        # The following assignment of 2 could be visible if this function has been called before.
        reveal_type(x)  # revealed: Literal[1, 2]
        x = 2
        # Once a binding is made in this scope, it shadows bindings from outer scopes.
        reveal_type(x)  # revealed: Literal[2]
    # From now on we assume `g` could be called at any time.
    reveal_type(x)  # revealed: Literal[1, 2]
```

The example above (hopefully) feels natural, but if we look at it closely, the reveals there are
making some beefy assumptions. For one, we assume `g` might be called before the final reveal, even
though in this case we can actually see that it's never called. A "sufficiently smart compiler"
could've narrowed that to `Literal[1]`, but we don't/can't track what functions are caled when, so
we're being conservative. On the other hand, the reveal of `Literal[2]` after the binding in `g` is
the opposite, an aggressive assumption that's not generally sound. Consider this counterexample
where `g` and `h` are siblings that both assign to `x`:

```py
def f():
    x = 1
    def g():
        nonlocal x
        x = 2
        reveal_type(x)  # revealed: Literal[2]
    def h():
        nonlocal x
        x = 3
        g()
        # The logic that gives us `Literal[2]` above also gives us `Literal[3]` here, even though
        # the call to `g()` means that `x` is in fact 2 at runtime. We can only reason locally about
        # these things; we can't do whole-program control flow analysis or solve the halting
        # problem. A fully sound typechecker would generally need to infer `Literal[2, 3]` both here
        # and above. That would be great here -- wrong answers are bad! -- but it would break too
        # much real-world code that expects `Literal[2]` in simple cases like `g` above.
        reveal_type(x)  # revealed: Literal[3]
    reveal_type(x)  # revealed: Literal[1, 2, 3]
```

So we're ok with making unsound assumptions to make simple, common cases do what users expect. Fine.
But then what about the reveal of `Literal[1, 2, 3]` at the end there? We aggressively shadow
bindings from outer or sibling scopes, but we conservatively include bindings from scopes nested
within the current one, once we've encountered them (i.e. in a top-to-bottom reading of the code).
This second behavior is _also_ unsound, because nested functions can "escape" the scope where
they're defined and affect reads on lines above their definition. But again these are the behaviors
that users expect. Here's the shadowing behavior in more detail:

```py
def foo():
    x = 2
    # We just defined `x`, and we haven't encountered any nested bindings of it yet.
    reveal_type(x)  # revealed: Literal[2]

    def bar():
        nonlocal x
        # We haven't encountered any local bindings for `x` yet. Its public type is visible to `bar`
        # here, including `bar`s own assignments of 3 below.
        reveal_type(x)  # revealed: Literal[2, 3, 4]

        x = 3
        # Local bindings shadow the whole public type.
        reveal_type(x)  # revealed: Literal[3]

    # We've encountered the nested assignment of 3, so we keep it visible alongside local bindings
    # in this scope.
    reveal_type(x)  # revealed: Literal[2, 3]

    x = 4
    # This assignment shadows the previous local bindings, but again nested bindings remain visible.
    reveal_type(x)  # revealed: Literal[3, 4]
```

## Nonlocal `+=` widening works like it does in loops

Using `+=` in a loop usually triggers fixpoint analysis, where after the list of `Literal` values
reaches an upper limit we widen the type to `int`. The same applies to `nonlocal` augmented
assignments, since the inner function body could run any number of times:

```py
def f():
    x = 1
    def g():
        nonlocal x
        x += 1
    reveal_type(x)  # revealed: int
```

## Nested `nonlocal` bindings are visible in intervening scopes

```py
def _():
    def _():
        def _():
            nonlocal x
            x = 1
        nonlocal x
        x = 2
        # The binding in `h` is visible here because it's nested under this scope, even though this
        # isn't the defining scope of `x`, and even though this scope doesn't declare `x` as
        # `nonlocal` (instead it uses it as a "free variable").
        reveal_type(x)  # revealed: Literal[1, 2]
    x = 3
    reveal_type(x)  # revealed: Literal[1, 2, 3]
```

## Conditional narrowing can filter out nested bindings

```py
def _():
    x = 42

    def hello():
        nonlocal x
        x = "hello"

    reveal_type(x)  # revealed: Literal[42, "hello"]

    if isinstance(x, int):
        reveal_type(x)  # revealed: Literal[42]
```

## Conditional `nonlocal` bindings leave parent scope bindings visible

Normal branching and merging rules apply to the shadowing behavior described in the previous
section:

```py
def flag(): ...
def foo():
    x = 2

    def bar():
        nonlocal x

        if flag():
            x = 3
            # The public types of `x` is shadowed here...
            reveal_type(x)  # revealed: Literal[3]

        # ...but still visible here.
        reveal_type(x)  # revealed: Literal[3, 2]
```

## Parameter defaults are evaluated before a function's body

We don't need to think about this ordering in normal execution, since the body of a function doesn't
get to cause any side effects until the function is called. But we do need to think about it in
inference, because of the (generally unsound) rule mentioned above about considering nested bindings
visible after we encounter them. That can matter in unusual situations like this one:

```py
def f():
    x = 1
    # This use of `x` doesn't see `x = 2` below.
    def g(y=reveal_type(x)):  # revealed: Literal[1]
        nonlocal x
        x = 2
```

## TODO: `nonlocal` writes in class scopes should be applied eagerly

Class bodies are evaluated eagerly, so nonlocal bindings in a class scope should behave more like
normal assignments. Currently we treat them like lazy nested bindings from function scopes, which
results in types that are wider than they should be:

```py
def f():
    x = 1

    class C:
        nonlocal x
        x = 2

    # TODO: Should be Literal[2].
    reveal_type(x)  # revealed: Literal[1, 2]
    x = 3
    # TODO: Should be Literal[3].
    reveal_type(x)  # revealed: Literal[2, 3]
```

However, nested bindings in function scopes within a class are still lazy:

```py
def f():
    x = 1

    class C:
        def g():
            nonlocal x
            x = 2

    reveal_type(x)  # revealed: Literal[1, 2]
    x = 3
    reveal_type(x)  # revealed: Literal[2, 3]
```

Similarly, class bodies within a function scope also behave lazily from the perspective of callers
of that function:

```py
def f():
    x = 1

    def g():
        class C:
            nonlocal x
            x = 2

    reveal_type(x)  # revealed: Literal[1, 2]
    x = 3
    reveal_type(x)  # revealed: Literal[2, 3]
```

## Large reachability constraint graphs fall back to `Unknown`

We have an "excessive complexity" cutoff beyond which we stop considering nested bindings
definitions for performance reasons, similar to how we handle loop header definitions:

```py
def f(flag: bool):
    # A bunch of random bindings of an unrelated variable to trigger the complexity cutoff.
    x = 0
    (
        flag and (x := 1),
        flag and (x := 2),
        flag and (x := 3),
        flag and (x := 4),
        flag and (x := 5),
        flag and (x := 6),
        flag and (x := 7),
        flag and (x := 8),
        flag and (x := 9),
        flag and (x := 10),
        flag and (x := 11),
        flag and (x := 12),
        flag and (x := 13),
        flag and (x := 14),
        flag and (x := 15),
        flag and (x := 16),
        flag and (x := 17),
        flag and (x := 18),
        flag and (x := 19),
        flag and (x := 20),
        flag and (x := 21),
        flag and (x := 22),
        flag and (x := 23),
        flag and (x := 24),
        flag and (x := 25),
        flag and (x := 26),
        flag and (x := 27),
        flag and (x := 28),
        flag and (x := 29),
        flag and (x := 30),
        flag and (x := 31),
        flag and (x := 32),
        flag and (x := 33),
        flag and (x := 34),
        flag and (x := 35),
        flag and (x := 36),
        flag and (x := 37),
        flag and (x := 38),
        flag and (x := 36),
        flag and (x := 36),
    )

    # Normally this `nonlocal` write would make us infer `int` for `y`, but now we ignore it.
    y = 0
    def g():
        nonlocal y
        y += 1
    reveal_type(y)  # revealed: Literal[0] | Unknown
```

## Free reads and narrowing constraints

Synthetic nested bindings definitions store both `global` and `nonlocal` nested writes, and we
decide which of them we respect at inference time. For scopes with "free" reads (i.e. uses but no
bindings in the current scope), we can _almost_ ignore the question of which nested bindings to
respect, because inference needs to walk to the defining scope to resolve the free read anyway.
However, respecting nested bindings in the current scope can still matter if the free read is
_narrowed_, because the nested bindings might not be. For example:

```py
def _():
    x: int | str = 1
    def _():
        def f1():
            nonlocal x
            x = "f1"
        if isinstance(x, int):
            def f2():
                nonlocal x
                x = "f2"
            # The narrowing condition covers the nested write from `f1` but not the one from `f2`. To
            # get this right, we have to detect that nested `nonlocal` writes are visible in this
            # scope.
            reveal_type(x)  # revealed: Literal["f2"] | int
```

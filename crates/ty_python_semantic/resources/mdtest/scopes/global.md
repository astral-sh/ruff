# `global` references

## Implicit global in function

A name reference to a never-defined symbol in a function is implicitly a global lookup.

```py
x = 1

def f():
    reveal_type(x)  # revealed: Literal[1]
```

## Explicit global in function

```py
x = 1

def f():
    global x
    reveal_type(x)  # revealed: Literal[1]
```

## Unassignable type in function

```py
x: int = 1

def f():
    y: int = 1
    # error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`"
    y = ""

    global x
    # error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`"
    x = ""

    global z
    # This binding is currently allowed, because the invalid declaration of `z` below acts like
    # `z: Unknown`. The end result is similar to what we get in the local case:
    #
    #     x = 42  # ok
    #     x: str  # error
    #
    # It would also be acceptable to emit an error here in the future. The important thing is that
    # at least one of the following two lines should fail.
    z = ""

# This declaration sees the synthetic definition for `z` that we add to this scope after the end of `f`.
# error: [invalid-declaration] "Cannot declare type `int` for inferred type `Literal[""]`"
z: int
```

## Narrowing

An assignment following a `global` statement should narrow the type in the local scope after the
assignment.

```py
x: int | None

def f():
    global x
    x = 1
    reveal_type(x)  # revealed: Literal[1]
```

Same for an `if` statement:

```py
x: int | None

def f():
    # The `global` keyword isn't necessary here, but this is testing that it doesn't get in the way
    # of narrowing.
    global x
    if x == 1:
        y: int = x  # allowed, because x cannot be None in this branch
```

## Nested function after conditional rebinding

A nested function should resolve a `global` name through the enclosing scope, even if that scope
conditionally rebinds it:

```py
x = 1

def outer(flag: bool) -> None:
    global x

    if flag:
        x = 2
        return

    def inner() -> None:
        reveal_type(x)  # revealed: Literal[1, 2]
```

## `nonlocal` and `global`

A binding cannot be both `nonlocal` and `global`. This should emit a semantic syntax error. CPython
marks the `nonlocal` line, while `mypy`, `pyright`, and `ruff` (`PLE0115`) mark the `global` line.

```py
x = 1

def f():
    x = 1
    def g() -> None:
        nonlocal x
        global x  # error: [invalid-syntax] "name `x` is nonlocal and global"
        x = None
```

## Global declaration after `global` statement

```py
def f():
    global x
    y = x
    x = 1  # No error.

x = 2
```

## Semantic syntax errors

Using a name prior to its `global` declaration in the same scope is a syntax error.

```py
x = 1
y = 2

def f():
    print(x)
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    global x
    print(x)
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    print(x)
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    global x, y
    print(x)
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    x = 1
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    x = 1

def f():
    global x
    x = 1
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    x = 1

def f():
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    global x, y
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    del x
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    global x
    del x
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    global x, y
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    print(f"{x=}")
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"

# still an error in module scope
x = None
global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
```

## Local bindings to a `global` shadow the public type

```py
x = 42

def f():
    global x
    reveal_type(x)  # revealed: Literal[42, "56"]
    x = "56"
    reveal_type(x)  # revealed: Literal["56"]
```

## Local assignment prevents falling back to the outer scope

```py
x = 42

def f():
    # error: [unresolved-reference] "Name `x` used when not defined"
    reveal_type(x)  # revealed: Unknown
    x = "56"
    reveal_type(x)  # revealed: Literal["56"]
```

## Annotating a `global` binding is a syntax error

```py
x: int = 1

def f():
    global x
    x: str = "foo"  # error: [invalid-syntax] "annotated name `x` can't be global"
```

However, `global` keywords are allowed (but useless) in the global scope, and it's not an error to
annotate the variable after that.

```py
global y

y: int = 42
```

## Global declarations affect the inferred type of the binding

Even if the `global` declaration isn't used in an assignment, we conservatively assume it could be:

```py
x = 1

def f():
    global x

# TODO: reveal_type(x)  # revealed: Unknown | Literal["1"]
```

## Global variables need an explicit definition in the global scope

You're allowed to use the `global` keyword to define new global variables that don't have any
explicit definition in the global scope, but we consider that fishy and prefer to lint on it:

```py
x = 1
y: int
# z is neither bound nor declared in the global scope

def f():
    global x, y, z  # error: [unresolved-global] "Invalid global declaration of `z`: `z` has no declarations or bindings in the global scope"
```

You don't need a definition for implicit globals, but you do for built-ins:

```py
def f():
    global __file__  # allowed, implicit global
    global int  # error: [unresolved-global] "Invalid global declaration of `int`: `int` has no declarations or bindings in the global scope"
```

## Nested class after global rebinding

Even if a `global` declaration is unresolved at module scope, nested eager scopes in the same
function should still see a rebinding that already happened:

```py
def factory():
    global x  # error: [unresolved-global] "Invalid global declaration of `x`: `x` has no declarations or bindings in the global scope"
    x = 1

    class C:
        reveal_type(x)  # revealed: Literal[1]
```

## References to variables before they are defined within a class scope are considered global

If we try to access a variable in a class before it has been defined, the lookup will fall back to
global.

```py
import secrets

x: str = "a"

def f(x: int, y: int):
    class C:
        reveal_type(x)  # revealed: int

    class D:
        x = None
        reveal_type(x)  # revealed: None

    class E:
        reveal_type(x)  # revealed: str
        x = None

        # error: [unresolved-reference]
        reveal_type(y)  # revealed: Unknown
        y = None

    # Declarations count as definitions, even if there's no binding.
    class F:
        reveal_type(x)  # revealed: str
        x: int
        reveal_type(x)  # revealed: str

    # Explicitly `nonlocal` variables don't count, even if they're bound.
    class G:
        nonlocal x
        reveal_type(x)  # revealed: int
        x = 42
        reveal_type(x)  # revealed: Literal[42]

    # Possibly-unbound variables get unioned with the fallback lookup.
    class H:
        if secrets.randbelow(2):
            x = None
        reveal_type(x)  # revealed: None | str
```

## A class body cannot see the class name being defined

The class name is bound only after the class body is evaluated, so a class body should not resolve
the class name to itself.

```py
class A:
    A = A  # error: [unresolved-reference]

B = 1

class B:
    reveal_type(B)  # revealed: Literal[1]
    B = B
```

## Visibility of `global` bindings from nested and sibling scopes

(`nonlocal` bindings behave similarly and have a similarly named test case in `nonlocal.md`.)

A `global` write from an inner scope can affect the target variable's inferred type in the global
scope defining scope. For the same reason, reads in a nested function can also see later bindings in
their own scope:

`global1.py`:

```py
x = 1

def f():
    global x
    # The following assignment of 2 could be visible if this function has been called before.
    reveal_type(x)  # revealed: Literal[1, 2]
    x = 2
    # Once a binding is made in this scope, it shadows bindings from outer scopes.
    reveal_type(x)  # revealed: Literal[2]

# From now on we assume `f` could be called at any time.
reveal_type(x)  # revealed: Literal[1, 2]
```

The example above (hopefully) feels natural, but if we look at it closely, the reveals there are
making some beefy assumptions. For one, we assume `f` might be called before the final reveal, even
though in this case we can actually see that it's never called. A "sufficiently smart compiler"
could've narrowed that to `Literal[1]`, but we don't/can't track what functions are caled when
(anywhere really, but especially not in the global scope), so we're being conservative. On the other
hand, the reveal of `Literal[2]` after the binding in `g` is the opposite, an aggressive assumption
that's not generally sound. Consider this counterexample where `f` and `g` are siblings that both
assign to `x`:

`global2.py`:

```py
x = 1

def f():
    global x
    x = 2
    reveal_type(x)  # revealed: Literal[2]

def g():
    global x
    x = 3
    f()
    # The logic that gives us `Literal[2]` above also gives us `Literal[3]` here, even though
    # the call to `f()` means that `x` is in fact 2 at runtime. We can only reason locally about
    # these things; we can't do whole-program control flow analysis or solve the halting
    # problem. A fully sound typechecker would generally need to infer `Literal[2, 3]` both here
    # and above. That would be great here -- wrong answers are bad! -- but it would break too
    # much real-world code that expects `Literal[2]` in simple cases like `f` above.
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

`global3.py`:

```py
x = 1
# We just defined `x`, and we haven't encountered any nested bindings of it yet.
reveal_type(x)  # revealed: Literal[1]

def bar():
    global x
    # We haven't encountered any local bindings for `x` yet. Its public type is visible to `bar`
    # here, including `bar`s own assignments of 3 below.
    reveal_type(x)  # revealed: Literal[1, 2, 3]

    x = 2
    # Local bindings shadow the whole public type.
    reveal_type(x)  # revealed: Literal[2]

# We've encountered the nested assignment of 3, so we keep it visible alongside local bindings
# in this scope.
reveal_type(x)  # revealed: Literal[1, 2]

x = 3
# This assignment shadows the previous local bindings, but again nested bindings remain visible.
reveal_type(x)  # revealed: Literal[2, 3]
```

## Nested `global` binding hidden by an enclosing local

A synthetic definition for the nested `global x` needs to flow through `outer`, so it can still
reach the module scope. But it should be ignored when resolving `x` in scopes where the name
resolves to `outer`'s local binding instead.

```py
x = 1

def outer():
    x = 2
    def inner():
        def writer():
            global x
            x = 3
        reveal_type(x)  # revealed: Literal[2]
    reveal_type(x)  # revealed: Literal[2]

reveal_type(x)  # revealed: Literal[1, 3]
```

## Global `+=` widening works like it does in loops

Using `+=` in a loop usually triggers fixpoint analysis, where after the list of `Literal` values
reaches an upper limit we widen the type to `int`. The same applies to `global` augmented
assignments, since the inner function body could run any number of times:

```py
x = 1

def f():
    global x
    x += 1

reveal_type(x)  # revealed: int
```

## Nested `global` bindings are visible in intervening scopes

```py
def _():
    def _():
        global x
        x = 1
    global x
    x = 2
    # The binding in the innermost function is visible here because it's nested under this scope,
    # even though this isn't the defining scope of `x`, and even though this scope doesn't declare `x`
    # as `global` (instead it uses it as a "free variable").
    reveal_type(x)  # revealed: Literal[1, 2]

x = 3
reveal_type(x)  # revealed: Literal[1, 2, 3]
```

## Conditional narrowing can filter out nested bindings

```py
x = 42

def hello():
    global x
    x = "hello"

reveal_type(x)  # revealed: Literal[42, "hello"]

if isinstance(x, int):
    reveal_type(x)  # revealed: Literal[42]
```

## Conditional `global` bindings leave parent scope bindings visible

Normal branching and merging rules apply to the shadowing behavior described in the previous
section:

```py
def flag(): ...

x = 1

def foo():
    global x

    x = 2

    def bar():
        global x

        if flag():
            x = 3
            # The public type of `x` is shadowed here...
            reveal_type(x)  # revealed: Literal[3]

        # ...but still visible here.
        reveal_type(x)  # revealed: Literal[3, 1, 2]
```

## Parameter defaults are evaluated before a function's body

We don't need to think about this ordering in normal execution, since the body of a function doesn't
get to cause any side effects until the function is called. But we do need to think about it in
inference, because of the (generally unsound) rule mentioned above about considering nested bindings
visible after we encounter them. That can matter in unusual sitautions like this one:

```py
x = 1

# This use of `x` doesn't see `x = 2` below.
def f(y=reveal_type(x)):  # revealed: Literal[1]
    global x
    x = 2
```

## TODO: `global` writes in class scopes should be applied eagerly

Class bodies are evaluated eagerly, so global bindings in a class scope should behave more like
normal assignments. Currently we treat them like lazy nested bindings from function scopes, which
results in types that are wider than they should be:

```py
x = 1

class C:
    global x
    x = 2

# TODO: Should be Literal[2].
reveal_type(x)  # revealed: Literal[1, 2]
x = 3
# TODO: Should be Literal[3].
reveal_type(x)  # revealed: Literal[2, 3]
```

However, nested bindings in function scopes within a class are still lazy:

```py
y = 1

class C:
    def f():
        global y
        y = 2

reveal_type(y)  # revealed: Literal[1, 2]
y = 3
reveal_type(y)  # revealed: Literal[2, 3]
```

Similarly, class bodies within a function scope also behave lazily from the perspective of callers
of that function:

```py
z = 1

def f():
    class C:
        global z
        z = 2

reveal_type(z)  # revealed: Literal[1, 2]
z = 3
reveal_type(z)  # revealed: Literal[2, 3]
```

## Interleaving `global` and local/`nonlocal` scopes using the same variable

`nonlocal` declarations aren't allowed to resolve to a scope with a `global` declaration for the
same variable; that's a semantic syntax error. That means that a `nonlocal` binding can't exactly
"flow through" a scope where the same name is `global`. (Similarly, free variable uses resolve when
they see a `global` declaration in an enclosing scope.) However, the reverse is possible: `global`
bindings can "flow through" intervening scopes where the same name is local/`nonlocal`, as long as
any `nonlocal` declarations in the picture get resolved legally.

So concretely, if a nested scope contributes a not-yet-resolved `nonlocal` binding, we can safely
assume that that binding is visible in the current scope. (The only way it wouldn't be is if the
current scope declares the same name `global`, but that's necessarily a semantic syntax error.)
However, if a nested scope has a `global` binding, we might not know yet whether it should be
visible in the current scope. If and only if we encounter a `global` declaration for that name
before any other uses, then the nested `global` binding is visible. But even in scopes where it's
not visible, it still needs to "flow through" to other scopes where it might be visible again,
including the global/module scope.

```py
x = 1

def _():
    def _():
        def global_middle():
            def _():
                def _():
                    def global_inner():
                        global x
                        x = 2
                    # The "free" case: we see the local `x` in the parent scope.
                    reveal_type(x)  # revealed: Literal[3]
                x = 3
            global x
            # The `global` case: we see the global `x`.
            reveal_type(x)  # revealed: Literal[2, 1]
        nonlocal x
        # The `nonlocal` case: we see the *other* local `x` in the parent scope.
        reveal_type(x)  # revealed: Literal[4, 5]
        x = 4
    x = 5

# The module case: we see the global `x`.
reveal_type(x)  # revealed: Literal[1, 2]
```

## Free reads and narrowing constraints

Synthetic nested bindings definitions store both `global` and `nonlocal` nested writes, and we
decide which of them we respect at inference time. For scopes with "free" reads (i.e. uses but no
bindings in the current scope), we can _almost_ ignore the question of which nested bindings to
respect, because inference needs to walk to the defining scope to resolve the free read anyway.
However, respecting nested bindings in the current scope can still matter if the free read is
_narrowed_, because the nested bindings might not be. For example:

```py
x: int | str = 1

def f():
    def g1():
        global x
        x = "g1"
    if isinstance(x, int):
        def g2():
            global x
            x = "g2"
        # The narrowing condition covers the nested write from `g1` but not the one from `g2`. To
        # get this right, we have to detect that nested `global` writes are visible in this scope.
        reveal_type(x)  # revealed: Literal["g2"] | int
```

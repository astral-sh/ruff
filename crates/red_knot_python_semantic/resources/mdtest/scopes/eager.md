# Eager scopes

Some scopes are executed eagerly: references to variables defined in enclosing scopes are resolved
_immediately_. This is in constrast to (for instance) function scopes, where those references are
resolved when the function is called.

## Function definitions

Function definitions are evaluated lazily.

```py
x = 1

def f():
    reveal_type(x)  # revealed: Unknown | Literal[2]

x = 2
```

## Class definitions

Class definitions are evaluated eagerly.

```py
def _():
    x = 1

    class A:
        reveal_type(x)  # revealed: Literal[1]

    x = 2
```

## List comprehensions

List comprehensions are evaluated eagerly.

```py
def _():
    x = 1

    # revealed: Literal[1]
    [reveal_type(x) for a in range(0)]

    x = 2
```

## Set comprehensions

Set comprehensions are evaluated eagerly.

```py
def _():
    x = 1

    # revealed: Literal[1]
    {reveal_type(x) for a in range(0)}

    x = 2
```

## Dict comprehensions

Dict comprehensions are evaluated eagerly.

```py
def _():
    x = 1

    # revealed: Literal[1]
    {a: reveal_type(x) for a in range(0)}

    x = 2
```

## Generator expressions

Generator expressions don't necessarily run eagerly, but in practice usually they do, so assuming
they do is the better default.

```py
def _():
    x = 1

    # revealed: Literal[1]
    list(reveal_type(x) for a in range(0))

    x = 2
```

## Lazy scopes are "sticky"

As we look through each enclosing scope when resolving a reference, lookups become lazy as soon as
we encounter any lazy scope, even if there are other eager scopes that enclose it.

### Eager scope within eager scope

If we don't encounter a lazy scope, lookup remains eager. The resolved binding is not necessarily in
the immediately enclosing scope. Here, the list comprehension and class definition are both eager
scopes, and we immediately resolve the use of `x` to (only) the `x = 1` binding.

```py
def _():
    x = 1

    class A:
        # revealed: Literal[1]
        [reveal_type(x) for a in range(0)]

    x = 2
```

### Eager scope within a lazy scope

The list comprehension is an eager scope, and it is enclosed within a function definition, which is
a lazy scope. Because we pass through this lazy scope before encountering any bindings or
definitions, the lookup is lazy.

```py
def _():
    x = 1

    def f():
        # revealed: Unknown | Literal[2]
        [reveal_type(x) for a in range(0)]

    x = 2
```

### Lazy scope within an eager scope

The function definition is a lazy scope, and it is enclosed within a class definition, which is an
eager scope. Even though we pass through an eager scope before encountering any bindings or
definitions, the lookup remains lazy.

```py
def _():
    x = 1

    class A:
        def f():
            # revealed: Unknown | Literal[2]
            reveal_type(x)

    x = 2
```

### Lazy scope within a lazy scope

No matter how many lazy scopes we pass through before encountering a binding or definition, the
lookup remains lazy.

```py
def _():
    x = 1

    def f():
        def g():
            # revealed: Unknown | Literal[2]
            reveal_type(x)

    x = 2
```

## Eager scope within a lazy scope within another eager scope

We have a list comprehension (eager scope), enclosed within a function definition (lazy scope),
enclosed within a class definition, all of which we must pass through before encountering any
binding of `x`. Even though the last scope we pass through is eager, the lookup is lazy, since we
encountered a lazy scope on the way.

```py
def _():
    x = 1

    class A:
        def f():
            # revealed: Unknown | Literal[2]
            [reveal_type(x) for a in range(0)]

    x = 2
```

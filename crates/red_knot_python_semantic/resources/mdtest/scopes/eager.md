# Eager scopes

Some scopes are executed eagerly: references to variables defined in enclosing scopes are resolved
_immediately_. This is in contrast to (for instance) function scopes, where those references are
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

        y = x

    x = 2

    reveal_type(A.y)  # revealed: Unknown | Literal[1]
```

## List comprehensions

List comprehensions are evaluated eagerly.

```py
def _():
    x = 1

    # revealed: Literal[1]
    [reveal_type(x) for a in range(1)]

    x = 2
```

## Set comprehensions

Set comprehensions are evaluated eagerly.

```py
def _():
    x = 1

    # revealed: Literal[1]
    {reveal_type(x) for a in range(1)}

    x = 2
```

## Dict comprehensions

Dict comprehensions are evaluated eagerly.

```py
def _():
    x = 1

    # revealed: Literal[1]
    {a: reveal_type(x) for a in range(1)}

    x = 2
```

## Generator expressions

Generator expressions don't necessarily run eagerly, but in practice usually they do, so assuming
they do is the better default.

```py
def _():
    x = 1

    # revealed: Literal[1]
    list(reveal_type(x) for a in range(1))

    x = 2
```

But that does lead to incorrect results when the generator expression isn't run immediately:

```py
def evaluated_later():
    x = 1

    # revealed: Literal[1]
    y = (reveal_type(x) for a in range(1))

    x = 2

    # The generator isn't evaluated until here, so at runtime, `x` will evaluate to 2, contradicting
    # our inferred type.
    print(next(y))
```

Though note that “the iterable expression in the leftmost `for` clause is immediately evaluated”
\[[spec][generators]\]:

```py
def iterable_evaluated_eagerly():
    x = 1

    # revealed: Literal[1]
    y = (a for a in [reveal_type(x)])

    x = 2

    # Even though the generator isn't evaluated until here, the first iterable was evaluated
    # immediately, so our inferred type is correct.
    print(next(y))
```

## Top-level eager scopes

All of the above examples behave identically when the eager scopes are directly nested in the global
scope.

### Class definitions

```py
x = 1

class A:
    reveal_type(x)  # revealed: Literal[1]

    y = x

x = 2

reveal_type(A.y)  # revealed: Unknown | Literal[1]
```

### List comprehensions

```py
x = 1

# revealed: Literal[1]
[reveal_type(x) for a in range(1)]

x = 2

# error: [unresolved-reference]
[y for a in range(1)]
y = 1
```

### Set comprehensions

```py
x = 1

# revealed: Literal[1]
{reveal_type(x) for a in range(1)}

x = 2

# error: [unresolved-reference]
{y for a in range(1)}
y = 1
```

### Dict comprehensions

```py
x = 1

# revealed: Literal[1]
{a: reveal_type(x) for a in range(1)}

x = 2

# error: [unresolved-reference]
{a: y for a in range(1)}
y = 1
```

### Generator expressions

```py
x = 1

# revealed: Literal[1]
list(reveal_type(x) for a in range(1))

x = 2

# error: [unresolved-reference]
list(y for a in range(1))
y = 1
```

`evaluated_later.py`:

```py
x = 1

# revealed: Literal[1]
y = (reveal_type(x) for a in range(1))

x = 2

# The generator isn't evaluated until here, so at runtime, `x` will evaluate to 2, contradicting
# our inferred type.
print(next(y))
```

`iterable_evaluated_eagerly.py`:

```py
x = 1

# revealed: Literal[1]
y = (a for a in [reveal_type(x)])

x = 2

# Even though the generator isn't evaluated until here, the first iterable was evaluated
# immediately, so our inferred type is correct.
print(next(y))
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
        [reveal_type(x) for a in range(1)]

    x = 2
```

### Class definition bindings are not visible in nested scopes

Class definitions are eager scopes, but any bindings in them are explicitly not visible to any
nested scopes. (Those nested scopes are typically (lazy) function definitions, but the rule also
applies to nested eager scopes like comprehensions and other class definitions.)

```py
def _():
    x = 1

    class A:
        x = 4

        # revealed: Literal[1]
        [reveal_type(x) for a in range(1)]

        class B:
            # revealed: Literal[1]
            [reveal_type(x) for a in range(1)]

    x = 2

x = 1

def _():
    class C:
        # revealed: Unknown | Literal[1]
        [reveal_type(x) for _ in [1]]
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
        [reveal_type(x) for a in range(1)]
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

### Eager scope within a lazy scope within another eager scope

We have a list comprehension (eager scope), enclosed within a function definition (lazy scope),
enclosed within a class definition (eager scope), all of which we must pass through before
encountering any binding of `x`. Even though the last scope we pass through is eager, the lookup is
lazy, since we encountered a lazy scope on the way.

```py
def _():
    x = 1

    class A:
        def f():
            # revealed: Unknown | Literal[2]
            [reveal_type(x) for a in range(1)]

    x = 2
```

## Annotations

Type annotations are sometimes deferred. When they are, the types that are referenced in an
annotation are looked up lazily, even if they occur in an eager scope.

### Eager annotations in a Python file

```py
from typing import ClassVar

x = int

class C:
    var: ClassVar[x]

reveal_type(C.var)  # revealed: int

x = str
```

### Deferred annotations in a Python file

```py
from __future__ import annotations

from typing import ClassVar

x = int

class C:
    var: ClassVar[x]

reveal_type(C.var)  # revealed: Unknown | str

x = str
```

### Deferred annotations in a stub file

```pyi
from typing import ClassVar

x = int

class C:
    var: ClassVar[x]

reveal_type(C.var)  # revealed: Unknown | str

x = str
```

[generators]: https://docs.python.org/3/reference/expressions.html#generator-expressions

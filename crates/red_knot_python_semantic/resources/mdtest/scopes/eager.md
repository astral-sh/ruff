# Some scopes are eagerly executed

## Comprehension scopes inside `for` loops

The list comprehension here is eagerly executed, so the `x` variable is definitely bound from the
perspective of the nested scope, even though it's potentially *unbound* from the perspective of code
after the `for` loop in the outer scope.

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

def f():
    for x in IntIterable():
        reveal_type(x)  # revealed: int

        # revealed: int
        [reveal_type(x) for _ in IntIterable()]

    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: int
```

## Eager scopes inside lazy scopes

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

def foo():
    for x in IntIterable():
        reveal_type(x)  # revealed: int
        def bar():
            # error: [possibly-unresolved-reference]
            # revealed: Unknown | int
            [reveal_type(x) for _ in IntIterable()]
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: int
```

## Lazy scopes inside eager scopes

Since the class definition is resolved eagerly, the first `reveal_type` only sees the `x = 1`
binding. However, the function inside of the class definition is resolved lazily, so it sees the
public type of `x`. Because `x` has no declared type, we currently widen the inferred type to
include `Unknown`.

(Put another way, the lazy scopes created for the two functions see the outer `x` in the same way,
even though one of them appears inside an eager class definition scope.)

```py
def f():
    x = 1

    class Foo:
        def in_class(self):
            reveal_type(x)  # revealed: Unknown | Literal[2]

    def outside_class(self):
        reveal_type(x)  # revealed: Unknown | Literal[2]
    x = 2
```

## Class scopes

Class definitions are evaluated eagerly, and see the bindings at the point of definition.

```py
def f():
    x = 1

    class Foo:
        reveal_type(x)  # revealed: Literal[1]

    x = 2
```

## Generator expressions

TODO Generator expressions don't necessarily run eagerly, but in practice usually they do, so
assuming they do is the better default:

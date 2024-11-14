# Some scopes are eagerly executed

## Comprehension scopes inside `for` loops

The list comprehension here is eagerly executed, so the `x` variable is definitely bound
from the perspective of the nested scope, even though it's potentially *unbound* from the
perspective of code after the `for` loop in the outer scope.

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

for x in IntIterable():
    reveal_type(x)  # revealed: int

    # revealed: int
    [reveal_type(x) for _ in IntIterable()]
```

## Class scopes

TODO class scopes also run eagerly:

## Generator expressions

TODO Generator expressions don't necessarily run eagerly, but in practice
usually they do, so assuming they do is the better default:

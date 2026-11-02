## What it does

Detects boolean conditions where variables typed as `Iterable`, `Iterator`, `Generator` or similar
are tested for their truthiness.

## Why is this bad?

Testing an `Iterable` object for truthiness strongly suggests that the code expects the object to
evaluate as falsy in a boolean context if it is empty. However, even empty `Iterable` objects can be
truthy if they do not define `__len__` or `__bool__`. The classic example of this is a generator:
unevaluated generators in Python are always truthy, even if they do not yield any elements at
runtime.

## Examples

```py
from collections.abc import Iterable


def process(items: Iterable[int]):
    if items:  # error: [truthiness-test-of-iterable]
        print("Received items")
    else:
        print("Didn't receive any items")


# prints "Received items", even though the passed-in generator is empty!
process(x for x in range(42) if x > 43)
```

If `process` in the above example does not need to accept generators, one solution is to rewrite the
annotation to use `Collection` instead of `Iterable`. `Collection` mandates that the object passed
in must define `__len__`, making a truthiness test much more likely to be meaningful:

```py
from collections.abc import Collection


def process(items: Collection[int]):
    if items:  # no diagnostic
        print("Received items")
    else:
        print("Didn't receive any items")


# passing in a generator is now rejected:
# error: [invalid-argument-type] "Expected `Collection[int]`, found `GeneratorType[int, None, None]`"
process(x for x in range(42) if x > 43)
```

If the function must also accept generators, another solution can be to collect the iterable into a
tuple or list before testing its length:

```py
def process(items: Iterable[int]):
    collected = tuple(items)
    if collected:  # no diagnostic
        print("Received items")
    else:
        print("Didn't receive any items")


# correctly prints "Didn't receive any items"
process(x for x in range(42) if x > 43)
```

## See also

- `redundant-condition` and `redundant-condition-strict` detect conditions that can be inferred as
    always being truthy or falsy
- `truthiness-test-of-callable` detects suspicious boolean tests where `Callable`-typed variables
    are tested for their truthiness

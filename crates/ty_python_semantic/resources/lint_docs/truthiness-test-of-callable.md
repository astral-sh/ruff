## What it does

Detects boolean conditions where `Callable` values are tested for their truthiness.

## Why is this bad?

`Callable`-typed variables are nearly always functions in practice, and functions are always truthy.
If `predicate` is a variable inferred as having a `Callable` type, therefore, a boolean test such as
`if predicate:` is usually not what you want; `if predicate()` (or similar) is usually what was
intended.

## Examples

```py
from collections.abc import Callable


def announce_if_ready(is_ready: Callable[[], bool]):
    if is_ready:  # error: [truthiness-test-of-callable]
        print("Ready")
```

You probably meant to call the value instead:

```py
def announce_if_ready_fixed(is_ready: Callable[[], bool]):
    if is_ready():  # no diagnostic
        print("Ready")
```

## See also

- `redundant-condition` and `redundant-condition-strict` detect conditions that can be inferred as
    always being truthy or falsy
- `truthiness-test-of-iterable` detects suspicious boolean tests where `Iterable`-typed variables
    are tested for their truthiness

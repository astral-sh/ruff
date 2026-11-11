## What it does

Checks for calls to a class whose `__new__` or `__init__` method may be missing.

## Why is this bad?

If the class defines a constructor method conditionally, Python may use an inherited method with a
different signature. Arguments accepted by the conditional method can then raise a `TypeError`.

## Examples

```python
def make_box(custom_init: bool):
    class Box:
        if custom_init:

            def __init__(self, value: int) -> None:
                self.value = value

    # Without the custom initializer, Box takes no arguments.
    return Box(1)  # error
```

## Rule history

- v0.0.102: Renamed from `possibly-missing-implicit-call`.

    `possibly-missing-implicit-call` remains as a compatibility alias for constructor calls.

    Prior to v0.0.102, subscript operations on objects with possibly missing subscript methods were
    also reported with this diagnostic code; these are now reported using `not-subscriptable` and/or
    `invalid-assignment` instead.

## What it does

Checks whether an assignment target can accept the value being stored.

Concretely, an assignment can only be accepted by ty if the target is writable and the value being
stored is [assignable to] the target's expected type.

## Why is this bad?

Python does not enforce variable annotations at runtime. Storing a string in a variable declared as
`int` can cause an error later when code tries to use it as a number.

Writing to a read-only target can fail at the assignment itself. For example, if an object has no
`__setitem__` method, assigning to a subscript on that object will raise `TypeError` at runtime. If
its class defines the method conditionally, the same assignment can succeed or fail depending on
whether the method is present at runtime.

## Examples

In this first example, the target (the name `retries`) is writable, but the value (a `str` instance)
isn't assignable to the expected type (`int`):

```python
retries: int = "3"  # error
```

Meanwhile, in this example the target is a subscript (`scores["Ada"]`). This is still a writable
target in this instance -- but again, the provided type (a `str`) does not match the target's
expected type (`int`):

```py
scores: dict[str, int] = {}
scores["Ada"] = "three"  # error
```

In this last example, the value assigned has the expected element type: `0` is an integer! But `seq`
could be an immutable `tuple` with no `__setitem__` method, so `invalid-assignment` is emitted here
to warn you that the target (`seq[0]`) might not be writable at all:

```python
def reset_first(seq: list[int] | tuple[int, ...]) -> None:
    if seq:
        seq[0] = 0  # error
```

[assignable to]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable

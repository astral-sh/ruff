## What it does

Checks for classes that inherit from a dataclass with `order=True`.

## Why is this bad?

When a dataclass has `order=True`, comparison methods (`__lt__`, `__le__`, `__gt__`, `__ge__`)
are generated that compare instances as tuples of their fields. These methods raise a
`TypeError` at runtime when comparing instances of different classes in the inheritance
hierarchy, even if one is a subclass of the other.

This violates the [Liskov Substitution Principle][liskov-substitution-principle] because child class instances cannot be
used in all contexts where parent class instances are expected.

## Example

```python
from dataclasses import dataclass


@dataclass(order=True)
class Parent:
    value: int


class Child(Parent):  # error
    pass


# At runtime, this raises TypeError:
# Child(1) < Parent(2)
```

Consider using [`functools.total_ordering`][total_ordering] instead, which does not have this limitation.

[liskov-substitution-principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
[total_ordering]: https://docs.python.org/3/library/functools.html#functools.total_ordering

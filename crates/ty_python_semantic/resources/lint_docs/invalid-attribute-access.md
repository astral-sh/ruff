## What it does

Checks for invalid attribute reads and writes.

This includes assignments to class variables from instances, assignments to
instance-only attributes from their class, and reads that definitely invoke a
descriptor with an invalid `__get__` method.

An "instance-only" variable is one which is only ever assigned to or declared
when accessed via `self` in an instance method.

## Why is this bad?

Incorrect assignments break the rules of the type system and
weaken a type checker's ability to accurately reason about your code.

## Examples

```python
from typing import ClassVar


class C:
    instance_var: int
    class_var: ClassVar[int] = 1

    def __init__(self):
        # instance variable declared in the class body
        self.instance_var = 42

        # instance-only variable not declared in the class body
        self.instance_only_var: int = 42


C.class_var = 3  # okay

C.instance_var = 56  # okay
C().instance_var = 72  # okay

C().instance_only_var = 100  # okay

# Cannot assign to class variable from instance
C().class_var = 3  # error

# Cannot assign to instance-only variable from class
C.instance_only_var = 56  # error
```

A descriptor's `__get__` method receives the descriptor, the instance (or
`None` for class access), and the owner class. An attribute read is invalid if
the method cannot accept those arguments:

```python
class Descriptor:
    def __get__(self) -> int:
        return 1


class C:
    value = Descriptor()


C().value  # error: [invalid-attribute-access]
```

We report a descriptor error only when the invalid call is definite. If the
attribute could instead contain a normal value or a valid descriptor, no
diagnostic is emitted.

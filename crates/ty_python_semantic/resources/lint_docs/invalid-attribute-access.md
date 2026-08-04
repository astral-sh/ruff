## What it does

Checks for assignments to class variables from instances
and assignments to instance-only attributes from their class.

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

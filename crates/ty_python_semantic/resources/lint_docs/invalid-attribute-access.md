## What it does

Checks for assignments to class variables from instances
and assignments to instance-only attributes from their class.

## Why is this bad?

Incorrect assignments break the rules of the type system and
weaken a type checker's ability to accurately reason about your code.

## Examples

```python
from typing import ClassVar


class C:
    class_var: ClassVar[int] = 1

    def __init__(self):
        self.instance_var: int = 42


C.class_var = 3  # okay
# Cannot assign to class variable
C().class_var = 3  # error
C().instance_var = 56
# Cannot assign to instance-only variable from class
C.instance_var = 56  # error
```
